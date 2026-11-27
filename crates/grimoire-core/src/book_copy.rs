//! Checked copies of books, keeping the source and the writer's local words safe.

use crate::projects_shelf::{free_destination, is_book, merge_copy_name};
use anyhow::{Context, Result, bail, ensure};
use chrono::{NaiveDate, NaiveDateTime};
use sha2::{Digest, Sha256};
use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::BuildHasher;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, Default)]
pub struct Progress {
    pub files: u64,
    pub bytes: u64,
}

/// Paths relative to the local book, including the names of parked scene copies.
#[derive(Debug, Default)]
pub struct MergeReport {
    pub added: Vec<PathBuf>,
    pub conflicts: Vec<PathBuf>,
    pub kept_local: Vec<PathBuf>,
}

/// Copy into a hidden sibling, check every file, then publish the finished book.
pub fn copy_book(
    source: &Path,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<()> {
    check_cancel(cancel, source)?;
    ensure!(is_book(source), "not a project: {}", source.display());
    ensure_absent(dest)?;
    let parent = parent_of(dest)?;
    let source_root = source
        .canonicalize()
        .with_context(|| source.display().to_string())?;
    let parent_root = parent
        .canonicalize()
        .with_context(|| parent.display().to_string())?;
    ensure!(
        !parent_root.starts_with(&source_root),
        "cannot copy a project into itself: {}",
        dest.display()
    );
    let entries = collect(source, false, cancel)?;
    let staging = Staging::new(parent)?;
    let mut total = Progress::default();
    for entry in &entries {
        let from = source.join(&entry.path);
        let to = staging.0.join(&entry.path);
        check_cancel(cancel, &from)?;
        if entry.directory {
            fs::create_dir(&to).with_context(|| format!("creating {}", to.display()))?;
        } else {
            copy_file(&from, &to, cancel, &mut total, progress)?;
        }
    }
    verify_entries(source, &entries, false, cancel)?;
    for entry in entries.iter().filter(|entry| !entry.directory) {
        verify_file(
            &source.join(&entry.path),
            &staging.0.join(&entry.path),
            cancel,
        )?;
    }
    check_cancel(cancel, dest)?;
    rename_new(&staging.0, dest)
}

/// Keep the old book beside the new one, moving it only after the copy is checked.
pub fn replace_with_copy(
    source: &Path,
    local: &Path,
    today: NaiveDate,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<PathBuf> {
    ensure!(is_book(local), "not a project: {}", local.display());
    separate_books(source, local)?;
    let parent = parent_of(local)?;
    let incoming = loop {
        let path = staging_path(parent);
        if metadata(&path)?.is_none() {
            break path;
        }
    };
    copy_book(source, &incoming, cancel, progress)?;
    let incoming = Staging(incoming);
    check_cancel(cancel, local)?;
    let name = local
        .file_name()
        .context("the local project needs a folder name")?;
    let saved = free_destination(
        parent,
        &format!(
            "{} (replaced {})",
            name.to_string_lossy(),
            today.format("%-d %b")
        ),
    );
    rename_new(local, &saved)?;
    if let Err(error) = rename_new(&incoming.0, local) {
        rename_new(&saved, local).with_context(|| {
            format!(
                "{error:#}; restoring {} from {} failed",
                local.display(),
                saved.display()
            )
        })?;
        return Err(error);
    }
    Ok(saved)
}

/// Add incoming words without changing existing files or the local .grimoire state.
pub fn merge_into(
    source: &Path,
    local: &Path,
    source_label: &str,
    when: NaiveDateTime,
    cancel: &AtomicBool,
) -> Result<MergeReport> {
    check_cancel(cancel, source)?;
    ensure!(is_book(source), "not a project: {}", source.display());
    ensure!(is_book(local), "not a project: {}", local.display());
    separate_books(source, local)?;
    let entries = collect(source, true, cancel)?;
    let staging = Staging::new(parent_of(local)?)?;
    let mut report = MergeReport::default();
    let mut copies: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut directories = Vec::new();
    for entry in &entries {
        let from = source.join(&entry.path);
        let to = local.join(&entry.path);
        check_cancel(cancel, &from)?;
        let existing = metadata(&to)?;
        if entry.directory {
            if let Some(meta) = existing {
                ensure!(meta.is_dir(), "not a regular directory: {}", to.display());
            }
            directories.push(to);
            continue;
        }
        let target = if let Some(meta) = existing {
            ensure!(meta.is_file(), "not a regular file: {}", to.display());
            if fingerprint(&from, cancel)? == fingerprint(&to, cancel)? {
                continue;
            }
            if entry.path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                report.kept_local.push(entry.path.clone());
                continue;
            }
            let stem = entry
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .with_context(|| format!("the scene needs a UTF-8 name: {}", from.display()))?;
            let base = merge_copy_name(stem, source_label, when);
            ensure!(
                Path::new(&base).file_name() == Some(std::ffi::OsStr::new(&base)),
                "invalid source label for {}",
                from.display()
            );
            let mut target = to.with_file_name(format!("{base}.md"));
            let mut n = 2;
            while metadata(&target)?.is_some()
                || copies.iter().any(|(_, path)| path == &target)
                || entries
                    .iter()
                    .any(|entry| local.join(&entry.path) == target)
            {
                target = to.with_file_name(format!("{base} {n}.md"));
                n += 1;
            }
            report
                .conflicts
                .push(target.strip_prefix(local)?.to_path_buf());
            target
        } else {
            report.added.push(entry.path.clone());
            to
        };
        let staged = staging.0.join(copies.len().to_string());
        copy_file(
            &from,
            &staged,
            cancel,
            &mut Progress::default(),
            &mut |_| {},
        )?;
        copies.push((from, target));
    }
    verify_entries(source, &entries, true, cancel)?;
    for (i, (from, _)) in copies.iter().enumerate() {
        verify_file(from, &staging.0.join(i.to_string()), cancel)?;
    }
    // Nothing local changes until every incoming file is checked. Roll back only
    // the files and directories created by this merge if publication is interrupted.
    let mut added = Additions::default();
    for dir in directories {
        check_cancel(cancel, &dir)?;
        if let Some(meta) = metadata(&dir)? {
            ensure!(meta.is_dir(), "not a regular directory: {}", dir.display());
        } else {
            fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
            added.paths.push((dir, true));
        }
    }
    for (i, (_, to)) in copies.iter().enumerate() {
        check_cancel(cancel, to)?;
        rename_new(&staging.0.join(i.to_string()), to)?;
        added.paths.push((to.clone(), false));
    }
    check_cancel(cancel, local)?;
    added.committed = true;
    Ok(report)
}

fn check_cancel(cancel: &AtomicBool, path: &Path) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "copy cancelled: {}",
        path.display()
    );
    Ok(())
}

fn parent_of(path: &Path) -> Result<&Path> {
    ensure!(
        path.file_name().is_some(),
        "a folder name is needed: {}",
        path.display()
    );
    Ok(path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new(".")))
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

fn ensure_absent(path: &Path) -> Result<()> {
    ensure!(
        metadata(path)?.is_none(),
        "already exists: {}",
        path.display()
    );
    Ok(())
}

fn separate_books(source: &Path, local: &Path) -> Result<()> {
    let from = source
        .canonicalize()
        .with_context(|| source.display().to_string())?;
    let to = local
        .canonicalize()
        .with_context(|| local.display().to_string())?;
    ensure!(
        !from.starts_with(&to) && !to.starts_with(&from),
        "project folders overlap: {} and {}",
        source.display(),
        local.display()
    );
    Ok(())
}

fn staging_path(parent: &Path) -> PathBuf {
    let hex = RandomState::new().hash_one(std::process::id());
    parent.join(format!(".wros-import-{hex:016x}"))
}

struct Staging(PathBuf);

impl Staging {
    fn new(parent: &Path) -> Result<Self> {
        loop {
            let path = staging_path(parent);
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(error).with_context(|| format!("creating {}", path.display()));
                }
            }
        }
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct Additions {
    paths: Vec<(PathBuf, bool)>,
    committed: bool,
}

impl Drop for Additions {
    fn drop(&mut self) {
        if !self.committed {
            for (path, directory) in self.paths.iter().rev() {
                if *directory {
                    let _ = fs::remove_dir(path);
                } else {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

#[derive(PartialEq, Eq)]
struct Entry {
    path: PathBuf,
    directory: bool,
}

fn collect(root: &Path, skip_state: bool, cancel: &AtomicBool) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let dir = root.join(&relative);
        check_cancel(cancel, &dir)?;
        ensure!(
            fs::symlink_metadata(&dir)
                .with_context(|| format!("reading {}", dir.display()))?
                .is_dir(),
            "not a regular directory: {}",
            dir.display()
        );
        for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry.with_context(|| format!("reading {}", dir.display()))?;
            let path = relative.join(entry.file_name());
            if skip_state && path == Path::new(".grimoire") {
                continue;
            }
            let kind = entry
                .file_type()
                .with_context(|| entry.path().display().to_string())?;
            if kind.is_dir() {
                if entry.file_name().to_string_lossy().starts_with(".wros-") {
                    continue;
                }
                pending.push(path.clone());
            } else if !kind.is_file() {
                continue;
            }
            entries.push(Entry {
                path,
                directory: kind.is_dir(),
            });
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn verify_entries(
    source: &Path,
    entries: &[Entry],
    skip_state: bool,
    cancel: &AtomicBool,
) -> Result<()> {
    let current = collect(source, skip_state, cancel)?;
    if entries != current.as_slice() {
        let i = entries
            .iter()
            .zip(&current)
            .position(|(a, b)| a != b)
            .unwrap_or(entries.len().min(current.len()));
        let entry = entries.get(i).or_else(|| current.get(i)).unwrap();
        bail!(
            "source changed while copying {}",
            source.join(&entry.path).display()
        );
    }
    Ok(())
}

fn open_regular(path: &Path) -> Result<fs::File> {
    ensure!(
        fs::symlink_metadata(path)
            .with_context(|| format!("reading {}", path.display()))?
            .is_file(),
        "not a regular file: {}",
        path.display()
    );
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .with_context(|| format!("reading {}", path.display()))?;
    ensure!(
        file.metadata()
            .with_context(|| format!("reading {}", path.display()))?
            .is_file(),
        "not a regular file: {}",
        path.display()
    );
    Ok(file)
}

fn copy_file(
    source: &Path,
    dest: &Path,
    cancel: &AtomicBool,
    total: &mut Progress,
    progress: &mut dyn FnMut(Progress),
) -> Result<()> {
    let mut input = open_regular(source)?;
    let mut output = fs::File::create_new(dest)
        .with_context(|| format!("copying {} to {}", source.display(), dest.display()))?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_cancel(cancel, source)?;
        let n = input
            .read(&mut buffer)
            .with_context(|| format!("reading {}", source.display()))?;
        if n == 0 {
            break;
        }
        output
            .write_all(&buffer[..n])
            .with_context(|| format!("writing {} from {}", dest.display(), source.display()))?;
        total.bytes += n as u64;
        progress(*total);
    }
    output
        .sync_all()
        .with_context(|| format!("flushing {} from {}", dest.display(), source.display()))?;
    total.files += 1;
    progress(*total);
    check_cancel(cancel, source)
}

fn fingerprint(path: &Path, cancel: &AtomicBool) -> Result<(u64, [u8; 32])> {
    let mut file = open_regular(path)?;
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_cancel(cancel, path)?;
        let n = file
            .read(&mut buffer)
            .with_context(|| format!("checking {}", path.display()))?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        hash.update(&buffer[..n]);
    }
    ensure!(
        file.metadata()
            .with_context(|| format!("checking {}", path.display()))?
            .len()
            == bytes,
        "size changed: {}",
        path.display()
    );
    Ok((bytes, hash.finalize().into()))
}

fn verify_file(source: &Path, dest: &Path, cancel: &AtomicBool) -> Result<()> {
    ensure!(
        fingerprint(source, cancel)? == fingerprint(dest, cancel)?,
        "copy did not match {}",
        source.display()
    );
    Ok(())
}

/// Rename a file or directory to a destination that must be new.
pub fn rename_new(from: &Path, to: &Path) -> Result<()> {
    // A destination appearing during the copy must not be replaced, even if empty.
    #[cfg(target_os = "linux")]
    let result = {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let source = CString::new(from.as_os_str().as_bytes())?;
        let dest = CString::new(to.as_os_str().as_bytes())?;
        // SAFETY: both paths are NUL-terminated and remain alive for the call.
        let status = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                dest.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    };
    #[cfg(not(target_os = "linux"))]
    let result = {
        ensure_absent(to)?;
        fs::rename(from, to)
    };
    result.with_context(|| format!("renaming {} to {}", from.display(), to.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicU64;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "grimoire-book-copy-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn book(&self, name: &str) -> PathBuf {
            let root = self.0.join(name);
            fs::create_dir_all(root.join("manuscript")).unwrap();
            root
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn put(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn visit(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let kind = entry.file_type().unwrap();
                if kind.is_dir() {
                    visit(root, &entry.path(), files);
                } else if kind.is_file() {
                    files.insert(
                        entry.path().strip_prefix(root).unwrap().to_path_buf(),
                        fs::read(entry.path()).unwrap(),
                    );
                }
            }
        }
        let mut files = BTreeMap::new();
        visit(root, root, &mut files);
        files
    }

    fn no_staging(parent: &Path) {
        for entry in fs::read_dir(parent).unwrap() {
            let entry = entry.unwrap();
            assert!(
                !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".wros-import-"),
                "staging left at {}",
                entry.path().display()
            );
        }
    }

    fn when() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 17)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap()
    }

    #[test]
    fn a_copied_book_matches_its_source_file_for_file() {
        let f = Fixture::new("copy");
        let source = f.book("Café says Hello World.");
        put(&source, "novel.toml", "title = 'Café'\n");
        put(
            &source,
            "manuscript/Part One/Scène One.md",
            "The first scene.\n",
        );
        put(
            &source,
            "manuscript/Part One/Scene Two.md",
            "The next scene.\n",
        );
        put(&source, "images/cover.bin", [0, 255, 1, 128]);
        put(&source, ".grimoire/sync.json", "local identity");
        fs::create_dir(source.join("empty folder")).unwrap();
        let expected = tree(&source);
        put(
            &source,
            ".wros-import-stale/partial.md",
            "not a finished copy",
        );
        put(
            &source,
            ".wros-drive-materialize-stale/partial.md",
            "unfinished",
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(source.join("novel.toml"), source.join("linked.toml")).unwrap();
            symlink(&source, source.join("linked folder")).unwrap();
            symlink(source.join("missing"), source.join("dangling")).unwrap();
        }
        let before = tree(&source);
        let dest = f.0.join("Imported Café");
        let cancel = AtomicBool::new(false);
        let mut updates = Vec::new();
        copy_book(&source, &dest, &cancel, &mut |p| {
            assert!(!dest.exists(), "only publish after checking");
            updates.push(p);
        })
        .unwrap();

        assert!(is_book(&dest));
        assert!(dest.join("empty folder").is_dir());
        assert_eq!(tree(&dest), expected);
        assert_eq!(tree(&source), before);
        assert!(!dest.join("linked.toml").exists());
        assert!(!dest.join("linked folder").exists());
        assert!(fs::symlink_metadata(dest.join("dangling")).is_err());
        let last = updates.last().unwrap();
        assert_eq!(last.files, expected.len() as u64);
        assert_eq!(
            last.bytes,
            expected
                .values()
                .map(|bytes| bytes.len() as u64)
                .sum::<u64>()
        );
        assert!(
            updates
                .windows(2)
                .all(|p| p[0].files <= p[1].files && p[0].bytes <= p[1].bytes)
        );
        no_staging(&f.0);
    }

    #[test]
    fn a_cancelled_or_failed_copy_leaves_nothing() {
        let f = Fixture::new("cancel-fail");
        let source = f.book("Source");
        put(&source, "manuscript/Locked Scene.md", "Words to keep.\n");
        let dest = f.0.join("Imported");
        let cancel = AtomicBool::new(true);
        assert!(copy_book(&source, &dest, &cancel, &mut |_| {}).is_err());
        assert!(!dest.exists());
        no_staging(&f.0);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let scene = source.join("manuscript/Locked Scene.md");
            fs::set_permissions(&scene, fs::Permissions::from_mode(0o000)).unwrap();
            // Root can still read chmod 000 files; it cannot exercise this failure.
            if fs::read(&scene).is_err() {
                cancel.store(false, Ordering::Relaxed);
                let result = copy_book(&source, &dest, &cancel, &mut |_| {});
                fs::set_permissions(&scene, fs::Permissions::from_mode(0o644)).unwrap();
                let error = result.unwrap_err();
                assert!(format!("{error:#}").contains("Locked Scene.md"));
                assert!(!dest.exists());
                no_staging(&f.0);
            } else {
                fs::set_permissions(&scene, fs::Permissions::from_mode(0o644)).unwrap();
            }
        }
    }

    #[test]
    fn copy_refuses_a_folder_that_is_not_a_book_or_an_existing_destination() {
        let f = Fixture::new("refuse");
        let source = f.book("Source");
        put(&source, "manuscript/Scene.md", "incoming");
        let plain = f.0.join("Notes");
        fs::create_dir(&plain).unwrap();
        let dest = f.0.join("Imported");
        let cancel = AtomicBool::new(false);
        assert!(copy_book(&plain, &dest, &cancel, &mut |_| {}).is_err());
        assert!(!dest.exists());
        fs::create_dir(&dest).unwrap();
        assert!(copy_book(&source, &dest, &cancel, &mut |_| {}).is_err());
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
        fs::remove_dir(&dest).unwrap();
        fs::write(&dest, "existing file").unwrap();
        assert!(copy_book(&source, &dest, &cancel, &mut |_| {}).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"existing file");
        #[cfg(unix)]
        {
            fs::remove_file(&dest).unwrap();
            std::os::unix::fs::symlink(f.0.join("missing"), &dest).unwrap();
            assert!(copy_book(&source, &dest, &cancel, &mut |_| {}).is_err());
            assert!(dest.is_symlink());
        }
        no_staging(&f.0);
    }

    #[test]
    fn replace_keeps_the_old_book_beside_the_new_one() {
        let f = Fixture::new("replace");
        let source = f.book("Source");
        let local = f.book("Book");
        put(&source, "manuscript/New Scene.md", "incoming words");
        put(&local, "manuscript/Old Scene.md", "original words");
        put(&local, ".grimoire/x", "old state");
        let incoming = tree(&source);
        let old = tree(&local);
        let today = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let saved = replace_with_copy(&source, &local, today, &AtomicBool::new(false), &mut |_| {
            assert_eq!(tree(&local), old)
        })
        .unwrap();
        assert_eq!(saved, f.0.join("Book (replaced 16 Nov)"));
        assert_eq!(tree(&local), incoming);
        assert_eq!(tree(&saved), old);
        assert_eq!(tree(&source), incoming);
        let again = replace_with_copy(&source, &local, today, &AtomicBool::new(false), &mut |_| {})
            .unwrap();
        assert_eq!(again, f.0.join("Book (replaced 16 Nov) 2"));
        assert_eq!(tree(&saved), old);
        no_staging(&f.0);
    }

    #[test]
    fn merge_adds_new_files_parks_differing_scenes_and_keeps_local_state() {
        let f = Fixture::new("merge");
        let source = f.book("Source");
        let local = f.book("Book");
        put(&source, "manuscript/Part One/Scene.md", "incoming scene");
        put(&local, "manuscript/Part One/Scene.md", "local scene");
        put(&source, "manuscript/Part Two/New.md", "extra scene");
        put(&local, "manuscript/Only Local.md", "local only");
        put(&source, "novel.toml", "title = 'Incoming'");
        put(&local, "novel.toml", "title = 'Local'");
        put(&source, ".grimoire/x", "incoming state");
        put(&source, ".grimoire/only-source", "must not import");
        put(&local, ".grimoire/x", "local state");
        put(&source, "images/cover.bin", [1, 2, 3]);
        put(&local, "images/cover.bin", [1, 2, 4]);
        put(&source, "notes/Identical.md", "same words");
        put(&local, "notes/Identical.md", "same words");
        let identical = local.join("notes/Identical.md");
        let modified = fs::metadata(&identical).unwrap().modified().unwrap();
        let before = tree(&source);
        let report = merge_into(
            &source,
            &local,
            "Google Drive",
            when(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let conflict =
            PathBuf::from("manuscript/Part One/Scene (from Google Drive, 2026-11-17 09-30).md");
        assert_eq!(
            report.added,
            vec![PathBuf::from("manuscript/Part Two/New.md")]
        );
        assert_eq!(report.conflicts, vec![conflict.clone()]);
        assert_eq!(
            report.kept_local,
            vec![
                PathBuf::from("images/cover.bin"),
                PathBuf::from("novel.toml")
            ]
        );
        assert_eq!(fs::read(local.join(&conflict)).unwrap(), b"incoming scene");
        assert_eq!(
            fs::read(local.join("manuscript/Part One/Scene.md")).unwrap(),
            b"local scene"
        );
        assert_eq!(
            fs::read(local.join("manuscript/Part Two/New.md")).unwrap(),
            b"extra scene"
        );
        assert_eq!(
            fs::read(local.join("manuscript/Only Local.md")).unwrap(),
            b"local only"
        );
        assert_eq!(
            fs::read(local.join("novel.toml")).unwrap(),
            b"title = 'Local'"
        );
        assert_eq!(fs::read(local.join(".grimoire/x")).unwrap(), b"local state");
        assert!(!local.join(".grimoire/only-source").exists());
        assert_eq!(
            fs::metadata(identical).unwrap().modified().unwrap(),
            modified
        );
        assert_eq!(tree(&source), before);
        assert_eq!(
            crate::sync::copy_of_path(&local.join(conflict))
                .unwrap()
                .original,
            "Scene"
        );
        no_staging(&f.0);
    }

    #[test]
    fn cancellation_during_a_file_copy_removes_staging() {
        let f = Fixture::new("mid-cancel");
        let source = f.book("Source");
        put(&source, "manuscript/Large.md", vec![b'x'; 256 * 1024]);
        let dest = f.0.join("Imported");
        let cancel = AtomicBool::new(false);
        let error = copy_book(&source, &dest, &cancel, &mut |p| {
            if p.bytes > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("Large.md"));
        assert!(!dest.exists());
        no_staging(&f.0);
    }

    #[test]
    fn verification_rejects_same_size_source_changes_and_removed_files() {
        for remove in [false, true] {
            let f = Fixture::new("changed-source");
            let source = f.book("Source");
            let scene = source.join("manuscript/Scene.md");
            put(&source, "manuscript/Scene.md", "before");
            let dest = f.0.join("Imported");
            let error = copy_book(&source, &dest, &AtomicBool::new(false), &mut |p| {
                if p.files == 1 {
                    if remove {
                        fs::remove_file(&scene).unwrap();
                    } else {
                        fs::write(&scene, "after!").unwrap();
                    }
                }
            })
            .unwrap_err();
            assert!(format!("{error:#}").contains("Scene.md"));
            assert!(!dest.exists());
            no_staging(&f.0);
        }
    }

    #[test]
    fn a_cancelled_replace_leaves_the_local_book_in_place() {
        let f = Fixture::new("replace-cancel");
        let source = f.book("Source");
        let local = f.book("Book");
        put(&source, "manuscript/Scene.md", "incoming");
        put(&local, "manuscript/Scene.md", "local");
        let before = tree(&local);
        let cancel = AtomicBool::new(false);
        assert!(
            replace_with_copy(&source, &local, when().date(), &cancel, &mut |p| {
                if p.files > 0 {
                    cancel.store(true, Ordering::Relaxed);
                }
            })
            .is_err()
        );
        assert_eq!(tree(&local), before);
        assert_eq!(fs::read_dir(&f.0).unwrap().count(), 2);
        no_staging(&f.0);
    }

    #[test]
    fn merge_never_overwrites_an_existing_conflict_copy() {
        let f = Fixture::new("merge-collision");
        let source = f.book("Source");
        let local = f.book("Book");
        put(&source, "manuscript/Scene.md", "incoming");
        put(&local, "manuscript/Scene.md", "local");
        let conflict = "manuscript/Scene (from Google Drive, 2026-11-17 09-30).md";
        put(&local, conflict, "previous import");
        let report = merge_into(
            &source,
            &local,
            "Google Drive",
            when(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(fs::read(local.join(conflict)).unwrap(), b"previous import");
        assert_eq!(report.conflicts.len(), 1);
        let parked = local.join(&report.conflicts[0]);
        assert_eq!(fs::read(&parked).unwrap(), b"incoming");
        assert_eq!(
            crate::sync::copy_of_path(&parked).unwrap().original,
            "Scene"
        );
        no_staging(&f.0);
    }

    #[test]
    fn a_cancelled_merge_changes_neither_book() {
        let f = Fixture::new("merge-cancel");
        let source = f.book("Source");
        let local = f.book("Book");
        put(&source, "manuscript/New.md", "incoming");
        let before = tree(&local);
        assert!(merge_into(&source, &local, "Laptop", when(), &AtomicBool::new(true)).is_err());
        assert_eq!(tree(&local), before);
        assert_eq!(
            fs::read(source.join("manuscript/New.md")).unwrap(),
            b"incoming"
        );
        no_staging(&f.0);
    }

    #[test]
    fn a_destination_created_during_copy_is_never_replaced() {
        let f = Fixture::new("late-destination");
        let source = f.book("Source");
        put(&source, "manuscript/Scene.md", "incoming");
        let dest = f.0.join("Imported");
        assert!(
            copy_book(&source, &dest, &AtomicBool::new(false), &mut |p| {
                if p.files == 1 {
                    fs::create_dir(&dest).unwrap();
                }
            })
            .is_err()
        );
        assert!(dest.is_dir());
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
        no_staging(&f.0);
    }

    #[test]
    fn a_failed_replace_keeps_the_local_book_and_removes_the_incoming_copy() {
        let f = Fixture::new("replace-fail");
        let source = f.book("Source");
        let local = f.book("Book");
        put(&source, "manuscript/Scene.md", "incoming");
        put(&local, "manuscript/Scene.md", "local");
        let before = tree(&local);
        let error = replace_with_copy(
            &source,
            &local,
            when().date(),
            &AtomicBool::new(false),
            &mut |p| {
                if p.files == 1 {
                    fs::remove_file(source.join("manuscript/Scene.md")).unwrap();
                }
            },
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("Scene.md"));
        assert_eq!(tree(&local), before);
        assert_eq!(fs::read_dir(&f.0).unwrap().count(), 2);
        no_staging(&f.0);
    }

    #[test]
    fn a_failed_merge_leaves_no_added_files_or_directories() {
        let f = Fixture::new("merge-fail");
        let source = f.book("Source");
        let local = f.book("Book");
        put(&source, "manuscript/New Part/Scene.md", "incoming");
        put(&source, "notes/Note.md", "a note");
        put(&local, "notes", "a file already uses this name");
        let incoming = tree(&source);
        let before = tree(&local);
        let error =
            merge_into(&source, &local, "Laptop", when(), &AtomicBool::new(false)).unwrap_err();
        assert!(format!("{error:#}").contains("notes"));
        assert_eq!(tree(&source), incoming);
        assert_eq!(tree(&local), before);
        assert!(!local.join("manuscript/New Part").exists());
        no_staging(&f.0);
    }

    #[test]
    fn overlapping_books_are_refused_without_changing_the_source() {
        let f = Fixture::new("overlap");
        let source = f.book("Source");
        let local = f.book("Source/Nested Book");
        put(&source, "manuscript/Scene.md", "source");
        put(&local, "manuscript/Scene.md", "nested");
        let before = tree(&source);
        let cancel = AtomicBool::new(false);
        assert!(copy_book(&source, &source.join("Imported"), &cancel, &mut |_| {}).is_err());
        assert!(replace_with_copy(&source, &local, when().date(), &cancel, &mut |_| {}).is_err());
        assert!(merge_into(&local, &source, "Laptop", when(), &cancel).is_err());
        assert_eq!(tree(&source), before);
        no_staging(&f.0);
        no_staging(&source);
    }

    #[test]
    #[cfg(unix)]
    fn merge_skips_source_symlinks_and_never_follows_local_symlinks_or_state() {
        use std::os::unix::fs::symlink;

        let f = Fixture::new("merge-links");
        let source = f.book("Source");
        let local = f.book("Book");
        let outside = f.book("Outside");
        put(&outside, "sentinel.md", "outside words");
        put(&source, ".grimoire/x", "incoming state");
        symlink(&outside, local.join(".grimoire")).unwrap();
        symlink(&outside, source.join("linked folder")).unwrap();
        symlink(outside.join("sentinel.md"), source.join("linked.md")).unwrap();
        merge_into(&source, &local, "USB", when(), &AtomicBool::new(false)).unwrap();
        assert!(!local.join("linked.md").exists());
        assert!(!local.join("linked folder").exists());
        assert!(!outside.join("x").exists());
        let before = tree(&outside);
        symlink(&outside, local.join("notes")).unwrap();
        put(&source, "notes/New.md", "incoming");
        assert!(merge_into(&source, &local, "USB", when(), &AtomicBool::new(false)).is_err());
        assert_eq!(tree(&outside), before);
        assert!(!local.join("notes/New.md").exists());
        no_staging(&f.0);
    }
}
