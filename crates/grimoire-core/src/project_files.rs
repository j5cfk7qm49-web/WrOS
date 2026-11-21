//! The names of a project's own files, and what they were called before.
//!
//! Until WrOS 0.1.0~rc22 a project was a "book": its settings were `novel.toml`, its id
//! `.grimoire/book-id` and its layout guide `Novel-Format.md`. Projects now use
//! `project.toml`, `.grimoire/project-id` and `Project-Format.md`. Everything reads the new
//! name first and the old one second, forever, so a Grimoire folder, a restored backup or a
//! copy from another computer still opens. The desk renames a project's files to the new
//! names when it opens it ([`migrate`]); going back to an older WrOS renames them back first
//! ([`restore_legacy`]).

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// One of a project's own files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// Title, author, targets and export details.
    Settings,
    /// The id its history is kept under.
    Id,
    /// The guide to how the project is laid out.
    Format,
}

impl FileKind {
    pub const ALL: [FileKind; 3] = [FileKind::Settings, FileKind::Id, FileKind::Format];

    /// Its name now, relative to the project folder.
    pub const fn name(self) -> &'static str {
        match self {
            FileKind::Settings => "project.toml",
            FileKind::Id => ".grimoire/project-id",
            FileKind::Format => "Project-Format.md",
        }
    }

    /// Its name before rc22.
    pub const fn legacy(self) -> &'static str {
        match self {
            FileKind::Settings => "novel.toml",
            FileKind::Id => ".grimoire/book-id",
            FileKind::Format => "Novel-Format.md",
        }
    }
}

/// Something is there, even a link that points nowhere.
fn present(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// Where a project's file is: the new name if it's there, else the old name if that is, else
/// the new name (where a new file goes).
pub fn path(root: &Path, kind: FileKind) -> PathBuf {
    let new = root.join(kind.name());
    if present(&new) {
        return new;
    }
    let old = root.join(kind.legacy());
    if present(&old) { old } else { new }
}

/// Where a new file of this kind goes. Nothing new is ever written under an old name.
pub fn new_path(root: &Path, kind: FileKind) -> PathBuf {
    root.join(kind.name())
}

/// The project still has a file under an old name.
pub fn has_legacy(root: &Path) -> bool {
    FileKind::ALL
        .iter()
        .any(|kind| present(&root.join(kind.legacy())))
}

/// What [`migrate`] did.
#[derive(Debug, Default, PartialEq)]
pub struct Migration {
    /// Renamed from the old name to the new one.
    pub renamed: Vec<FileKind>,
    /// Both names were there: the new one stays and the old one went to the Trash, here.
    pub trashed: Vec<(FileKind, PathBuf)>,
    /// Why a file kept its old name (read-only, say). It opens as before and is tried again
    /// next time.
    pub kept_old: Option<String>,
}

impl Migration {
    pub fn is_empty(&self) -> bool {
        self.renamed.is_empty() && self.trashed.is_empty() && self.kept_old.is_none()
    }
}

/// Rename a project's old file names to the new ones. A rename never reads the file, so a
/// file that can't be read is renamed all the same. Where both names are there (a copy from an
/// older WrOS arrived after the rename), the new one stays and the old one goes to the Trash,
/// from where it can be restored.
pub fn migrate(root: &Path) -> Migration {
    let mut done = Migration::default();
    for kind in FileKind::ALL {
        let old = root.join(kind.legacy());
        if !present(&old) {
            continue;
        }
        let new = root.join(kind.name());
        let result = if present(&new) {
            crate::project::trash(root, &old).map(|went| done.trashed.push((kind, went)))
        } else {
            fs::rename(&old, &new)
                .map(|()| done.renamed.push(kind))
                .map_err(anyhow::Error::from)
        };
        if let Err(error) = result
            && done.kept_old.is_none()
        {
            done.kept_old = Some(format!("{}: {error:#}", kind.legacy()));
        }
    }
    done
}

/// Put the old names back, for an older WrOS that reads only those. Returns how many files
/// were renamed. A file already under its old name, or with an old-named file beside it, is
/// left alone.
pub fn restore_legacy(root: &Path) -> Result<usize> {
    let mut count = 0;
    for kind in FileKind::ALL {
        let (new, old) = (root.join(kind.name()), root.join(kind.legacy()));
        if present(&new) && !present(&old) {
            fs::rename(&new, &old)
                .with_context(|| format!("renaming {} back to {}", kind.name(), kind.legacy()))?;
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch project folder, removed again when the test ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn project() -> Scratch {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "grimoire-project-files-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".grimoire")).unwrap();
        fs::create_dir_all(dir.join("manuscript")).unwrap();
        Scratch(dir)
    }

    fn old_names(root: &Path) {
        fs::write(root.join("novel.toml"), "title = \"Old\"\n").unwrap();
        fs::write(root.join(".grimoire/book-id"), "abc123\n").unwrap();
        fs::write(root.join("Novel-Format.md"), "# Format\n").unwrap();
    }

    #[test]
    fn path_prefers_the_new_name_then_the_old() {
        let dir = project();
        let root = dir.path();
        assert_eq!(path(root, FileKind::Settings), root.join("project.toml"));
        fs::write(root.join("novel.toml"), "").unwrap();
        assert_eq!(path(root, FileKind::Settings), root.join("novel.toml"));
        fs::write(root.join("project.toml"), "").unwrap();
        assert_eq!(path(root, FileKind::Settings), root.join("project.toml"));
        assert_eq!(
            new_path(root, FileKind::Id),
            root.join(".grimoire/project-id")
        );
    }

    #[test]
    fn migrate_renames_all_three() {
        let dir = project();
        let root = dir.path();
        old_names(root);
        assert!(has_legacy(root));
        let done = migrate(root);
        assert_eq!(done.renamed, FileKind::ALL);
        assert!(done.trashed.is_empty() && done.kept_old.is_none());
        assert_eq!(
            fs::read_to_string(root.join("project.toml")).unwrap(),
            "title = \"Old\"\n"
        );
        assert_eq!(
            fs::read_to_string(root.join(".grimoire/project-id")).unwrap(),
            "abc123\n"
        );
        assert!(root.join("Project-Format.md").is_file());
        assert!(!has_legacy(root));
        assert!(migrate(root).is_empty(), "a second open has nothing to do");
    }

    #[test]
    fn migrate_with_both_keeps_the_new_and_trashes_the_old() {
        let dir = project();
        let root = dir.path();
        fs::write(root.join("project.toml"), "title = \"New\"\n").unwrap();
        fs::write(root.join("novel.toml"), "title = \"Stray\"\n").unwrap();
        let done = migrate(root);
        assert!(done.renamed.is_empty());
        let (kind, went) = &done.trashed[0];
        assert_eq!(*kind, FileKind::Settings);
        assert_eq!(fs::read_to_string(went).unwrap(), "title = \"Stray\"\n");
        assert!(went.starts_with(crate::project::trash_dir(root)));
        assert_eq!(
            crate::project::trashed_from(root, went),
            Some(root.join("novel.toml")),
            "Restore puts it back where it was"
        );
        assert_eq!(
            fs::read_to_string(root.join("project.toml")).unwrap(),
            "title = \"New\"\n"
        );
        assert!(!root.join("novel.toml").exists());
    }

    #[cfg(unix)]
    #[test]
    fn migrate_renames_an_unreadable_file_without_reading_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project();
        let root = dir.path();
        fs::write(root.join("novel.toml"), "title = \"Secret\"\n").unwrap();
        fs::set_permissions(root.join("novel.toml"), fs::Permissions::from_mode(0o000)).unwrap();
        let done = migrate(root);
        assert_eq!(done.renamed, [FileKind::Settings]);
        let new = root.join("project.toml");
        assert_eq!(new.metadata().unwrap().permissions().mode() & 0o777, 0);
        fs::set_permissions(&new, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(fs::read_to_string(new).unwrap(), "title = \"Secret\"\n");
    }

    #[cfg(unix)]
    #[test]
    fn migrate_on_a_read_only_folder_reports_and_changes_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project();
        let root = dir.path();
        old_names(root);
        let locked = fs::Permissions::from_mode(0o555);
        fs::set_permissions(root, locked.clone()).unwrap();
        fs::set_permissions(root.join(".grimoire"), locked).unwrap();
        // Root can rename anything; the test means something only for a writer.
        let probe = root.join("probe");
        if fs::write(&probe, "").is_ok() {
            let _ = fs::remove_file(probe);
            for p in [root.to_path_buf(), root.join(".grimoire")] {
                fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
            }
            return;
        }
        let done = migrate(root);
        assert!(done.renamed.is_empty());
        assert!(
            done.kept_old
                .as_deref()
                .unwrap()
                .starts_with("novel.toml: ")
        );
        assert!(root.join("novel.toml").is_file() && root.join("Novel-Format.md").is_file());
        for p in [root.to_path_buf(), root.join(".grimoire")] {
            fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn restore_legacy_puts_old_names_back_and_counts() {
        let dir = project();
        let root = dir.path();
        old_names(root);
        migrate(root);
        assert_eq!(restore_legacy(root).unwrap(), 3);
        assert!(root.join("novel.toml").is_file());
        assert!(root.join(".grimoire/book-id").is_file());
        assert!(root.join("Novel-Format.md").is_file());
        assert!(!root.join("project.toml").exists());
        assert_eq!(restore_legacy(root).unwrap(), 0, "nothing left to rename");

        // A project made new has no format guide; one with both names is left as it is.
        let dir = project();
        let root = dir.path();
        fs::write(root.join("project.toml"), "new").unwrap();
        fs::write(root.join(".grimoire/project-id"), "id").unwrap();
        fs::write(root.join(".grimoire/book-id"), "stray").unwrap();
        assert_eq!(restore_legacy(root).unwrap(), 1);
        assert_eq!(
            fs::read_to_string(root.join(".grimoire/book-id")).unwrap(),
            "stray"
        );
    }

    #[test]
    fn migrate_then_restore_round_trips_byte_for_byte() {
        let dir = project();
        let root = dir.path();
        old_names(root);
        let before: Vec<_> = FileKind::ALL
            .iter()
            .map(|k| fs::read(root.join(k.legacy())).unwrap())
            .collect();
        migrate(root);
        restore_legacy(root).unwrap();
        let after: Vec<_> = FileKind::ALL
            .iter()
            .map(|k| fs::read(root.join(k.legacy())).unwrap())
            .collect();
        assert_eq!(before, after);
    }
}
