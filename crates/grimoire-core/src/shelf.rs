//! Where books are kept, and a book as one file.
//!
//! A book is a folder of plain Markdown files, which is what lets it sync,
//! keep its history and be read without Grimoire. A folder is harder to
//! find, send and put somewhere safe than a file is, so:
//!
//! - new books go in one **books folder** (`~/Documents`)
//!   unless the writer says where;
//! - **a copy** is the whole book as one `.zip`, named for the book and the
//!   day, in `Copies` inside the books folder (or wherever it's asked for);
//! - Grimoire **opens a copy**: it's unpacked into the books folder as a
//!   book of its own, never over anything, and the copy is left as it was;
//! - a book kept somewhere else can be **moved** into the books folder.

use crate::project_files::{self, FileKind};
use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

/// The folder new books go in when nobody says otherwise.
pub fn home() -> PathBuf {
    crate::paths::home().join("Documents")
}

/// Where copies of books are saved, inside the books folder.
pub fn copies_dir(home: &Path) -> PathBuf {
    home.join("Copies")
}

/// The books in the books folder, the one worked on most recently first.
/// (Brought over from another computer, they're found without being told.)
pub fn books_in(home: &Path) -> Vec<PathBuf> {
    let touched = |root: &Path| {
        [
            root.join(".grimoire/resume.md"),
            project_files::path(root, FileKind::Settings),
            root.join("manuscript"),
        ]
        .iter()
        .filter_map(|p| fs::metadata(p).ok()?.modified().ok())
        .max()
    };
    let mut found: Vec<(Option<std::time::SystemTime>, PathBuf)> = fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .filter(|p| is_book(p))
        .map(|p| (touched(&p), p))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().map(|(_, p)| p).collect()
}

/// Is this folder a book?
pub fn is_book(p: &Path) -> bool {
    crate::projects_shelf::is_book(p)
}

/// Does this look like a copy of a book: a `.zip` file?
pub fn is_copy(p: &Path) -> bool {
    p.is_file()
        && p.extension()
            .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("zip"))
}

/// `dir/name`, or `dir/name 2`, `name 3`… when that's taken: something new
/// never lands on something that's there.
pub fn unique(dir: &Path, name: &str) -> PathBuf {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && e.eq_ignore_ascii_case("zip") => {
            (s.to_string(), format!(".{e}"))
        }
        _ => (name.to_string(), String::new()),
    };
    let mut path = dir.join(name);
    let mut n = 2;
    while path.symlink_metadata().is_ok() {
        path = dir.join(format!("{stem} {n}{ext}"));
        n += 1;
    }
    path
}

/// What a book is on disk, in numbers a person can check.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Facts {
    pub files: usize,
    pub bytes: u64,
}

/// Every file and directory of the book, in a steady order, as (path on disk,
/// path inside the book, whether it is a directory). Grimoire's own leftovers that a copy
/// has no use for are left out: temporary files mid-save, and the words
/// recovery is holding (they're in the scenes once they're saved).
fn entries_of(root: &Path) -> Result<Vec<(PathBuf, String, bool)>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, String, bool)>) -> Result<()> {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let path = e.path();
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let inside: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            let name = inside.last().cloned().unwrap_or_default();
            if name.ends_with(".tmp") || name == ".DS_Store" {
                continue;
            }
            if inside.first().map(String::as_str) == Some(".grimoire")
                && inside.get(1).map(String::as_str) == Some("recovery")
            {
                continue;
            }
            // A link is followed only to a file: a folder linked from
            // somewhere else isn't part of the book.
            let kind = e.file_type()?;
            if kind.is_dir() {
                out.push((path.clone(), inside.join("/"), true));
                walk(root, &path, out)?;
            } else if path.is_file() {
                out.push((path, inside.join("/"), false));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

/// How many files the book is and how much room they take.
pub fn facts(root: &Path) -> Facts {
    let mut f = Facts::default();
    for (path, _, directory) in entries_of(root).unwrap_or_default() {
        if !directory {
            f.files += 1;
            f.bytes += fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        }
    }
    f
}

/// A name for the book that's safe in a file name anywhere, and still reads
/// as its title.
fn file_name_for(title: &str, root: &Path) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        root.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Project".into())
    } else {
        cleaned
    }
}

/// Save the whole book at `root` as one file in `into`: `<Title> <date>.zip`
/// (a number after it if there's one from today already). Inside is the
/// book's own folder, so unpacking it by hand gives the book back too. The
/// file appears only once it's complete. Returns where it is.
pub fn save_copy(
    root: &Path,
    title: &str,
    into: &Path,
    today: chrono::NaiveDate,
) -> Result<PathBuf> {
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    if !is_book(root) {
        bail!("{} isn't a project", root.display());
    }
    fs::create_dir_all(into).with_context(|| format!("creating {}", into.display()))?;
    let name = file_name_for(title, root);
    let name_with_date = format!("{name} {}.zip", today.format("%Y-%m-%d"));
    let folder = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| name.clone());

    let mut entries = entries_of(root)?;
    // A portable copy must not claim the original project's Drive folder.
    entries.retain(|(_, inside, _)| !inside.eq_ignore_ascii_case(".grimoire/sync.json"));
    let mut tmp = tempfile::Builder::new()
        .prefix(".copy-")
        .suffix(".tmp")
        .tempfile_in(into)
        .with_context(|| format!("creating a temporary copy in {}", into.display()))?;
    {
        let mut z = zip::ZipWriter::new(io::BufWriter::new(tmp.as_file_mut()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (path, inside, directory) in &entries {
            let name = format!("{folder}/{inside}");
            if *directory {
                z.add_directory(name, opts)?;
            } else {
                let mut file =
                    fs::File::open(path).with_context(|| format!("reading {}", path.display()))?;
                z.start_file(name, opts)?;
                io::copy(&mut file, &mut z)?;
            }
        }
        z.finish()?.flush()?;
    }
    // Checked before it's given its name: every entry is in it.
    let kept = fs::File::open(tmp.path())
        .ok()
        .and_then(|f| zip::ZipArchive::new(f).ok())
        .map(|z| z.len());
    if kept != Some(entries.len()) {
        bail!("the copy came out incomplete, so it wasn't kept");
    }
    // The temporary name is removed on drop, after the copy is published.
    publish_copy(
        tmp.path(),
        into,
        &name_with_date,
        unique(into, &name_with_date),
    )
}

fn publish_copy(tmp: &Path, into: &Path, name: &str, mut dest: PathBuf) -> Result<PathBuf> {
    loop {
        match fs::hard_link(tmp, &dest) {
            Ok(()) => return Ok(dest),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => dest = unique(into, name),
            Err(_) => {
                // Some filesystems don't support hard links.
                let dest = unique(into, name);
                fs::rename(tmp, &dest).with_context(|| format!("naming {}", dest.display()))?;
                return Ok(dest);
            }
        }
    }
}

/// The newest copy of this book in `dir`, if there is one: its file and
/// when it was made.
pub fn newest_copy(
    dir: &Path,
    title: &str,
    root: &Path,
) -> Option<(PathBuf, std::time::SystemTime)> {
    let lead = format!("{} ", file_name_for(title, root));
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with(&lead) && n.to_lowercase().ends_with(".zip")
        })
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .max_by_key(|(_, when)| *when)
}

/// The most a copy may unpack to. A book of a million words is a few
/// megabytes; this is only here so a file that isn't a project can't fill a disk.
const MOST: u64 = 4 * 1024 * 1024 * 1024;

/// Open a copy: unpack the book in `copy` into `home` as a book of its own,
/// under its own folder name (numbered if that's taken). Nothing already
/// there is touched, and the copy stays as it was. Returns the new book.
pub fn open_copy(copy: &Path, home: &Path) -> Result<PathBuf> {
    let not_ours = || {
        format!(
            "{} isn't a copy of a Grimoire project",
            copy.file_name().unwrap_or_default().to_string_lossy()
        )
    };
    let file = fs::File::open(copy).with_context(|| format!("opening {}", copy.display()))?;
    let mut z = zip::ZipArchive::new(&file).with_context(not_ours)?;
    reject_duplicate_names(&file, z.central_directory_start())?;

    // Every entry, by a path that can't climb out of where it's put.
    let mut entries = Vec::new();
    let mut total: u64 = 0;
    for i in 0..z.len() {
        let e = z.by_index(i)?;
        let Some(path) = e.enclosed_name() else {
            bail!(
                "{}: it holds an entry that would land outside the project",
                not_ours()
            );
        };
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!(
                "{}: it holds an entry that would land outside the project",
                not_ours()
            );
        }
        total = total.saturating_add(e.size());
        if total > MOST {
            bail!("{}: it's far too big to be a project", not_ours());
        }
        entries.push((path, e.is_dir(), e.name().to_string()));
    }
    let top = copy_root(&entries).with_context(not_ours)?;
    let name = match top.file_name() {
        Some(n) => n.to_string_lossy().to_string(),
        None => copy
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Project".into()),
    };
    fs::create_dir_all(home).with_context(|| format!("creating {}", home.display()))?;
    // Unpack beside the destination so a failed extraction leaves no half-project.
    let staging = tempfile::Builder::new()
        .prefix(".opening-")
        .tempdir_in(home)?;
    for (i, (path, directory, name)) in entries.iter().enumerate() {
        let Ok(rel) = path.strip_prefix(&top) else {
            continue;
        };
        if rel
            .parent()
            .is_some_and(|p| p.as_os_str().eq_ignore_ascii_case(".grimoire"))
            && rel
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("sync.json"))
        {
            continue;
        }
        let to = staging.path().join(rel);
        if *directory {
            fs::create_dir_all(&to)?;
            continue;
        }
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&to)
            .with_context(|| {
                if let Some((_, _, first)) = entries[..i].iter().find(|(path, directory, _)| {
                    !directory
                        && path
                            .strip_prefix(&top)
                            .is_ok_and(|rel| same_location(&staging.path().join(rel), &to))
                }) {
                    return format!(
                        "copy entries {first:?} and {name:?} would write the same file"
                    );
                }
                format!("writing {}", to.display())
            })?;
        io::copy(&mut z.by_index(i)?, &mut file)?;
    }
    // A hand-made archive can identify an empty project by its settings alone.
    fs::create_dir_all(staging.path().join("manuscript"))?;
    if !is_book(staging.path()) {
        bail!(not_ours());
    }
    publish_project(staging.path(), home, &name)
}

/// ZipArchive indexes entries by raw name, so check the central directory
/// before extraction for repeated names that the index would hide.
fn reject_duplicate_names(mut file: &fs::File, start: u64) -> Result<()> {
    file.seek(SeekFrom::Start(start))?;
    let mut names = HashSet::new();
    let mut header = [0; 46]; // Fixed part of a ZIP central directory file header.
    loop {
        file.read_exact(&mut header[..4])?;
        if &header[..4] != b"PK\x01\x02" {
            return Ok(());
        }
        file.read_exact(&mut header[4..])?;
        let length = |i| u16::from_le_bytes([header[i], header[i + 1]]);
        let mut name = vec![0; usize::from(length(28))];
        file.read_exact(&mut name)?;
        if names.contains(&name) {
            let name = String::from_utf8_lossy(&name);
            bail!("copy entries {name:?} and {name:?} would write the same file");
        }
        names.insert(name);
        file.seek(SeekFrom::Current(
            i64::from(length(30)) + i64::from(length(32)),
        ))?;
    }
}

/// Prefer a project at the archive's top, then a folder with its own manuscript.
/// A wrapper named "manuscript" can itself have a child "manuscript".
fn copy_root(entries: &[(PathBuf, bool, String)]) -> Option<PathBuf> {
    let settings = |name: &std::ffi::OsStr| {
        name == FileKind::Settings.name() || name == FileKind::Settings.legacy()
    };
    let mut top_settings = false;
    let mut top_manuscript = false;
    let mut only_manuscript = true;
    let mut nested_settings = None;
    let mut nested_manuscript = None;
    for (path, dir, _) in entries {
        let mut parts = path.iter();
        let Some(first) = parts.next() else {
            continue;
        };
        let second = parts.next();
        let third = parts.next();
        top_settings |= !dir && second.is_none() && settings(first);
        only_manuscript &= first == "manuscript";
        top_manuscript |= first == "manuscript" && (*dir || second.is_some());
        if let Some(second) = second {
            if !dir && third.is_none() && settings(second) {
                nested_settings.get_or_insert(first);
            }
            if second == "manuscript" && (*dir || third.is_some()) {
                nested_manuscript.get_or_insert(first);
            }
        }
    }
    let wrapped = nested_manuscript.or(nested_settings);
    let wrapper_named_manuscript = only_manuscript && wrapped.is_some_and(|p| p == "manuscript");
    if top_settings || (top_manuscript && !wrapper_named_manuscript) {
        Some(PathBuf::new())
    } else {
        wrapped.map(PathBuf::from)
    }
}

fn same_location(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (a.metadata(), b.metadata()) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }
}

fn publish_project(from: &Path, home: &Path, name: &str) -> Result<PathBuf> {
    loop {
        let dest = unique(home, name);
        match crate::book_copy::rename_new(from, &dest) {
            Ok(()) => return Ok(dest),
            // Another open (or move) claimed this name after unique().
            Err(_) if dest.symlink_metadata().is_ok() => {}
            Err(e) => return Err(e),
        }
    }
}

/// Move the book at `root` into `home`, under the same folder name
/// (numbered if that's taken). One rename: the book is in one place or the
/// other, never half in both. A book on another disk can't be moved that
/// way and is left where it is; a copy opened in the books folder does it
/// instead. Returns where the book is now.
pub fn move_to(root: &Path, home: &Path) -> Result<PathBuf> {
    if !is_book(root) {
        bail!("{} isn't a project", root.display());
    }
    let here = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    fs::create_dir_all(home).with_context(|| format!("creating {}", home.display()))?;
    let home_real = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    if here.parent() == Some(home_real.as_path()) {
        bail!("it's already in your projects folder");
    }
    if home_real.starts_with(&here) {
        bail!("the projects folder is inside this project, so it can't move there");
    }
    let name = here
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .context("this project's folder has no name")?;
    publish_project(&here, &home_real, &name).map_err(|e| {
        anyhow::anyhow!(
            "it couldn't be moved in one step ({e}), so it's left where it is. \
             Save a copy and open the copy instead: that puts it in your projects folder"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("grimoire-shelf-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn book(dir: &Path, name: &str) -> PathBuf {
        let root = dir.join(name);
        let ch = root.join("manuscript/01-Act-One/01-Chapter-One");
        fs::create_dir_all(&ch).unwrap();
        fs::write(root.join("novel.toml"), "title = \"Ash & Ember: A Tale\"\n").unwrap();
        fs::write(
            ch.join("01-Gravel.md"),
            "---\ntitle: Gravel\n---\n\nThe rain came sideways.\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("characters")).unwrap();
        fs::write(
            root.join("characters/01-Wren.md"),
            "Wren keeps the ledgers.\n",
        )
        .unwrap();
        fs::create_dir_all(root.join(".grimoire/recovery")).unwrap();
        fs::write(root.join(".grimoire/recovery/held.md"), "unsaved").unwrap();
        fs::write(root.join(".grimoire/book-id"), "abc123\n").unwrap();
        root
    }

    fn day() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
    }

    #[test]
    fn shelf_home_is_wros_projects_folder() {
        assert_eq!(home(), crate::paths::home().join("Documents"));
    }

    #[test]
    fn zip_copy_of_project_toml_project_opens_as_project() {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().join("Documents");
        let root = book(&home, "Ash and Ember");
        let settings = root.join("project.toml");
        fs::rename(root.join("novel.toml"), &settings).unwrap();
        assert!(is_book(&root));
        assert_eq!(books_in(&home), vec![root.clone()]);
        assert_eq!(facts(&root).files, 4);
        let sync = ".grimoire/sync.json";
        fs::write(root.join(sync), "original Drive identity").unwrap();

        let copy = save_copy(&root, "Ash", &copies_dir(&home), day()).unwrap();
        let opened = open_copy(&copy, &home).unwrap();
        assert!(is_book(&opened));
        assert_eq!(
            fs::read_to_string(project_files::path(&opened, FileKind::Settings)).unwrap(),
            fs::read_to_string(settings).unwrap(),
        );
        assert!(!opened.join("novel.toml").exists());
        assert!(!opened.join(sync).exists());
        assert!(root.join(sync).exists());
    }

    #[test]
    fn two_copies_same_day_are_two_files() {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().join("Documents");
        let root = book(&home, "Grimoire");
        book(&home, "Another Project");
        let copies = copies_dir(&home);
        let first = save_copy(&root, "Ash", &copies, day()).unwrap();
        let second = save_copy(&root, "Ash", &copies, day()).unwrap();
        assert_eq!(first, copies.join("Ash 2026-11-16.zip"));
        assert_eq!(second, copies.join("Ash 2026-11-16 2.zip"));
        assert!(first.is_file() && second.is_file());
        assert_eq!(fs::read_dir(&copies).unwrap().count(), 2);
        let archive = zip::ZipArchive::new(fs::File::open(second).unwrap()).unwrap();
        assert!(!archive.is_empty());
        assert!(
            archive
                .file_names()
                .all(|name| name.starts_with("Grimoire/"))
        );
    }

    #[test]
    fn open_copy_never_replaces_existing_folder() {
        let d = tempfile::tempdir().unwrap();
        let root = book(d.path(), "Ash and Ember");
        let home = d.path().join("Documents");
        let copy = save_copy(&root, "Ash", &copies_dir(&home), day()).unwrap();
        let existing = home.join("Ash and Ember");
        fs::create_dir(&existing).unwrap();
        assert_eq!(
            open_copy(&copy, &home).unwrap(),
            home.join("Ash and Ember 2")
        );
        assert_eq!(fs::read_dir(&existing).unwrap().count(), 0);
        assert!(copy.is_file());
    }

    #[test]
    fn open_copy_refuses_two_entries_for_one_file() {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().join("Documents");
        let copy = d.path().join("Ash.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&copy).unwrap());
        // ZIP 8.6 requires distinct raw names. These both name the same file
        // after path normalization, even on a case-sensitive filesystem.
        let first = "manuscript/01-Dawn.md";
        let second = "manuscript//01-Dawn.md";
        for (name, body) in [(first, b"Dawn"), (second, b"dawn")] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();

        let error = open_copy(&copy, &home).unwrap_err().to_string();
        assert!(error.contains(first) && error.contains(second), "{error}");
        assert_eq!(fs::read_dir(&home).unwrap().count(), 0);

        // Write distinct spellings, then give the second entry the exact same
        // name in both ZIP headers. ZipWriter rejects raw duplicates itself.
        let second = "manuscript/01-dawn.md";
        let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        for name in [first, second] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"draft").unwrap();
        }
        let mut bytes = zip.finish().unwrap().into_inner();
        let offsets: Vec<_> = bytes
            .windows(second.len())
            .enumerate()
            .filter_map(|(i, name)| (name == second.as_bytes()).then_some(i))
            .collect();
        assert_eq!(offsets.len(), 2);
        for i in offsets {
            bytes[i..i + first.len()].copy_from_slice(first.as_bytes());
        }
        fs::write(&copy, bytes).unwrap();
        let error = open_copy(&copy, &home).unwrap_err().to_string();
        assert_eq!(error.matches(first).count(), 2, "{error}");
        assert_eq!(fs::read_dir(home).unwrap().count(), 0);
    }

    #[test]
    fn copy_publish_never_replaces_an_existing_copy() {
        let d = tempfile::tempdir().unwrap();
        let copies = copies_dir(&d.path().join("Documents"));
        fs::create_dir_all(&copies).unwrap();
        let name = "Ash 2026-11-16.zip";
        let dest = unique(&copies, name);
        let tmp = copies.join(".copy.tmp");
        fs::write(&tmp, "new copy").unwrap();
        // A second writer claims the name once the first has chosen it.
        fs::write(&dest, "existing copy").unwrap();
        let published = publish_copy(&tmp, &copies, name, dest.clone()).unwrap();
        assert_eq!(published, copies.join("Ash 2026-11-16 2.zip"));
        assert_eq!(fs::read_to_string(dest).unwrap(), "existing copy");
        assert_eq!(fs::read_to_string(published).unwrap(), "new copy");
    }

    #[test]
    fn copy_round_trip_with_empty_manuscript_and_a_folder_named_manuscript() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("manuscript");
        let home = d.path().join("Documents");
        fs::create_dir_all(root.join("manuscript")).unwrap();
        fs::write(root.join("project.toml"), "title = \"Ash\"\n").unwrap();
        let copy = save_copy(&root, "Ash", &copies_dir(&home), day()).unwrap();
        let opened = open_copy(&copy, &home).unwrap();
        assert_eq!(opened, home.join("manuscript"));
        assert_eq!(fs::read_dir(opened.join("manuscript")).unwrap().count(), 0);
        assert!(opened.join("project.toml").is_file());
        assert!(!opened.join("manuscript/project.toml").exists());

        let hand = d.path().join("settings-only.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&hand).unwrap());
        zip.start_file("project.toml", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"title = \"Blank\"\n").unwrap();
        zip.finish().unwrap();
        let opened = open_copy(&hand, &home).unwrap();
        assert_eq!(opened, home.join("settings-only"));
        assert!(is_book(&opened));
    }

    #[test]
    fn a_copy_is_one_file_and_opens_as_the_same_book() {
        let d = tmp("round");
        let root = book(&d, "Ash and Ember");
        let home = d.join("Documents");
        let copy = save_copy(&root, "Ash & Ember: A Tale", &copies_dir(&home), day()).unwrap();
        // Named for the book and the day, with nothing a file name can't hold.
        assert_eq!(
            copy.file_name().unwrap().to_string_lossy(),
            "Ash & Ember A Tale 2026-11-16.zip"
        );
        assert!(is_copy(&copy));
        // No temporary file is left beside it.
        assert_eq!(fs::read_dir(copies_dir(&home)).unwrap().count(), 1);

        let opened = open_copy(&copy, &home).unwrap();
        assert_eq!(opened, home.join("Ash and Ember"));
        assert!(is_book(&opened));
        let scene = "manuscript/01-Act-One/01-Chapter-One/01-Gravel.md";
        assert_eq!(
            fs::read_to_string(opened.join(scene)).unwrap(),
            fs::read_to_string(root.join(scene)).unwrap()
        );
        assert!(opened.join("characters/01-Wren.md").is_file());
        assert!(
            opened.join(".grimoire/book-id").is_file(),
            "its history id comes too"
        );
        assert!(
            !opened.join(".grimoire/recovery").exists(),
            "words held for recovery aren't part of a copy"
        );
        // The copy is still there, and the original is untouched.
        assert!(copy.is_file() && root.join(scene).is_file());
    }

    #[test]
    fn nothing_is_ever_written_over() {
        let d = tmp("unique");
        let root = book(&d, "Ash and Ember");
        let home = d.join("Documents");
        let first = save_copy(&root, "Ash", &copies_dir(&home), day()).unwrap();
        let second = save_copy(&root, "Ash", &copies_dir(&home), day()).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            second.file_name().unwrap().to_string_lossy(),
            "Ash 2026-11-16 2.zip"
        );
        // Opened twice: two books, the second numbered.
        let a = open_copy(&first, &home).unwrap();
        fs::write(a.join("novel.toml"), "title = \"changed\"\n").unwrap();
        let b = open_copy(&first, &home).unwrap();
        assert_eq!(b, home.join("Ash and Ember 2"));
        assert_eq!(
            fs::read_to_string(a.join("novel.toml")).unwrap(),
            "title = \"changed\"\n"
        );
        // The newest copy is found by the book's name.
        assert_eq!(
            newest_copy(&copies_dir(&home), "Ash", &root).map(|c| c.0.is_file()),
            Some(true)
        );
        assert!(newest_copy(&copies_dir(&home), "Another Book", &root).is_none());
    }

    #[test]
    fn a_zip_that_is_not_a_book_is_refused_and_leaves_nothing() {
        let d = tmp("refuse");
        let home = d.join("Documents");
        // Not a zip at all.
        let junk = d.join("notes.zip");
        fs::write(&junk, "not a zip").unwrap();
        assert!(open_copy(&junk, &home).is_err());
        // A zip with no manuscript in it.
        let other = d.join("photos.zip");
        {
            let mut z = zip::ZipWriter::new(fs::File::create(&other).unwrap());
            z.start_file(
                "holiday/readme.txt",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            z.write_all(b"hello").unwrap();
            z.finish().unwrap();
        }
        let err = open_copy(&other, &home).unwrap_err().to_string();
        assert!(err.contains("isn't a copy of a Grimoire project"), "{err}");
        let left: Vec<_> = fs::read_dir(&home)
            .map(|r| r.flatten().collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "nothing half-made is left: {left:?}");
    }

    #[test]
    fn the_inside_of_a_book_zipped_by_hand_opens_too() {
        let d = tmp("inside");
        let home = d.join("Documents");
        let hand = d.join("My Novel.zip");
        {
            let mut z = zip::ZipWriter::new(fs::File::create(&hand).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("manuscript/01-One/01-Scene.md", o).unwrap();
            z.write_all(b"Once.").unwrap();
            z.start_file("novel.toml", o).unwrap();
            z.write_all(b"title = \"My Novel\"\n").unwrap();
            z.finish().unwrap();
        }
        let opened = open_copy(&hand, &home).unwrap();
        assert_eq!(opened, home.join("My Novel"));
        assert!(opened.join("manuscript/01-One/01-Scene.md").is_file());
    }

    #[test]
    fn the_books_folder_lists_its_books_and_nothing_else() {
        let d = tmp("list");
        let home = d.join("Documents");
        assert!(books_in(&home).is_empty(), "no folder yet is no books");
        book(&home, "Ash and Ember");
        book(&home, "The Long Night");
        let copies = copies_dir(&home);
        fs::create_dir_all(&copies).unwrap();
        fs::write(copies.join("Ash 2026-11-16.zip"), "a copy").unwrap();
        fs::write(home.join("notes.txt"), "not a book").unwrap();
        let mut names: Vec<String> = books_in(&home)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, ["Ash and Ember", "The Long Night"]);
    }

    #[test]
    fn a_book_moves_into_the_books_folder_whole() {
        let d = tmp("move");
        let root = book(&d.join("code/project"), "Ash and Ember");
        let home = d.join("Documents");
        let before = facts(&root);
        assert_eq!(before.files, 4, "recovery's held words aren't counted");
        let now = move_to(&root, &home).unwrap();
        assert_eq!(now.file_name().unwrap(), "Ash and Ember");
        assert!(now.starts_with(fs::canonicalize(&home).unwrap()));
        assert!(!root.exists(), "it's in one place, not two");
        assert_eq!(facts(&now), before);
        // Already there: said, and nothing moves.
        let err = move_to(&now, &home).unwrap_err().to_string();
        assert!(err.contains("already in your projects folder"), "{err}");
        // A second book of the same name lands beside it, numbered.
        let twin = book(&d.join("elsewhere"), "Ash and Ember");
        let moved = move_to(&twin, &home).unwrap();
        assert_eq!(moved.file_name().unwrap(), "Ash and Ember 2");
    }
}
