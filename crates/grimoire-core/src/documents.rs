//! Exclusive in-process document identities for editable panes.
//!
//! A registry is NOT a filesystem lock or a replacement for guarded saves.
//! It prevents this UI from constructing two authoritative editors for one
//! canonical file (including symlink and Unix hard-link aliases). Manuscript
//! bytes still belong to Project/Node and its existing save/recovery paths.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Pane {
    #[default]
    Primary,
    Secondary,
}
impl Pane {
    pub fn other(self) -> Self {
        match self {
            Self::Primary => Self::Secondary,
            Self::Secondary => Self::Primary,
        }
    }
    fn index(self) -> usize {
        if self == Self::Primary { 0 } else { 1 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assignment {
    Assigned,
    /// Focus this existing editor instead; do not allocate another buffer.
    OwnedBy(Pane),
}

#[derive(Debug, Clone)]
struct Identity {
    relative: PathBuf,
    canonical: PathBuf,
    file_id: Option<(u64, u64)>,
}
impl Identity {
    fn same_file(&self, other: &Self) -> bool {
        self.canonical == other.canonical
            || (self.file_id.is_some() && self.file_id == other.file_id)
    }
}

/// One owner per existing document. Failed assignments do not release a pane's
/// previous lease. Missing files retain their identity until the existing
/// Project logic has handled temporary disappearance/deletion and recovery.
pub struct Registry {
    root: PathBuf,
    owners: [Option<Identity>; 2],
}
impl Registry {
    pub fn new(root: &Path) -> Result<Self> {
        let root = fs::canonicalize(root).context("resolving document registry root")?;
        if !root.is_dir() {
            bail!("document registry root is not a directory");
        }
        Ok(Self {
            root,
            owners: [None, None],
        })
    }

    fn identify(&self, relative: &Path, cached: Option<&Identity>) -> Result<Identity> {
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!("editable document path must be relative and stay inside the project");
        }
        let canonical = match fs::canonicalize(self.root.join(relative)) {
            Ok(path) => path,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(previous) = cached.filter(|p| p.relative == relative) {
                    return Ok(previous.clone());
                }
                return Err(e).context("document is missing; no new editor created");
            }
            Err(e) => return Err(e).context("cannot verify editable document identity"),
        };
        if !canonical.starts_with(&self.root) {
            bail!("editable document resolves outside the project");
        }
        let metadata = fs::metadata(&canonical).context("reading document identity")?;
        if !metadata.is_file() {
            bail!("editable document must be a regular file");
        }
        #[cfg(unix)]
        let file_id = {
            use std::os::unix::fs::MetadataExt;
            Some((metadata.dev(), metadata.ino()))
        };
        #[cfg(not(unix))]
        let file_id = None;
        Ok(Identity {
            relative: relative.to_path_buf(),
            canonical,
            file_id,
        })
    }

    pub fn owner_for(&self, relative: &Path) -> Result<Option<Pane>> {
        let cached = self
            .owners
            .iter()
            .flatten()
            .find(|v| v.relative == relative);
        let identity = self.identify(relative, cached)?;
        for pane in [Pane::Primary, Pane::Secondary] {
            if let Some(owner) = &self.owners[pane.index()] {
                let refreshed = self.identify(&owner.relative, Some(owner))?;
                if identity.same_file(&refreshed) {
                    return Ok(Some(pane));
                }
            }
        }
        Ok(None)
    }

    pub fn assign(&mut self, pane: Pane, relative: &Path) -> Result<Assignment> {
        let identity = self.identify(relative, self.owners[pane.index()].as_ref())?;
        if let Some(other) = &self.owners[pane.other().index()] {
            let refreshed = self.identify(&other.relative, Some(other))?;
            if identity.same_file(&refreshed) {
                return Ok(Assignment::OwnedBy(pane.other()));
            }
        }
        self.owners[pane.index()] = Some(identity);
        Ok(Assignment::Assigned)
    }

    pub fn release(&mut self, pane: Pane) {
        self.owners[pane.index()] = None;
    }
    pub fn path(&self, pane: Pane) -> Option<&Path> {
        self.owners[pane.index()]
            .as_ref()
            .map(|v| v.relative.as_path())
    }

    /// Recheck before saving: external symlink/hard-link changes must not turn
    /// two previously distinct open documents into competing writers.
    pub fn validate(&mut self) -> Result<()> {
        let mut refreshed = self.owners.clone();
        for owner in refreshed.iter_mut().flatten() {
            *owner = self.identify(&owner.relative, Some(owner))?;
        }
        if let (Some(a), Some(b)) = (&refreshed[0], &refreshed[1])
            && a.same_file(b)
        {
            bail!("open documents now alias one file; saving paused, both buffers retained");
        }
        self.owners = refreshed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "writingdeck-documents-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::write(path.join("one.md"), "First document\n").unwrap();
            fs::write(path.join("two.md"), "Second document\n").unwrap();
            Self(path)
        }
        fn registry(&self) -> Registry {
            Registry::new(&self.0).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn distinct_documents_have_distinct_owners_and_same_file_focuses_existing_owner() {
        let f = Fixture::new();
        let mut r = f.registry();
        assert_eq!(
            r.assign(Pane::Primary, Path::new("one.md")).unwrap(),
            Assignment::Assigned
        );
        assert_eq!(
            r.assign(Pane::Secondary, Path::new("one.md")).unwrap(),
            Assignment::OwnedBy(Pane::Primary)
        );
        assert_eq!(r.path(Pane::Secondary), None);
        assert_eq!(
            r.assign(Pane::Secondary, Path::new("two.md")).unwrap(),
            Assignment::Assigned
        );
        assert_eq!(
            r.assign(Pane::Primary, Path::new("two.md")).unwrap(),
            Assignment::OwnedBy(Pane::Secondary)
        );
        assert_eq!(r.path(Pane::Primary), Some(Path::new("one.md")));
        r.validate().unwrap();
    }
    #[test]
    fn unsafe_missing_and_nonfile_paths_cannot_displace_an_existing_editor() {
        let f = Fixture::new();
        let mut r = f.registry();
        r.assign(Pane::Primary, Path::new("one.md")).unwrap();
        for path in ["", "../one.md", "/etc/passwd", "missing.md", "."] {
            assert!(r.assign(Pane::Primary, Path::new(path)).is_err(), "{path}");
            assert_eq!(r.path(Pane::Primary), Some(Path::new("one.md")));
        }
    }
    #[test]
    fn absent_registered_file_keeps_its_lease_for_existing_recovery_handling() {
        let f = Fixture::new();
        let mut r = f.registry();
        r.assign(Pane::Primary, Path::new("one.md")).unwrap();
        fs::remove_file(f.0.join("one.md")).unwrap();
        r.validate().unwrap();
        assert_eq!(r.path(Pane::Primary), Some(Path::new("one.md")));
        assert!(r.assign(Pane::Secondary, Path::new("one.md")).is_err());
        r.release(Pane::Primary);
        assert_eq!(r.path(Pane::Primary), None);
    }
    #[cfg(unix)]
    #[test]
    fn symbolic_and_hardlink_aliases_cannot_acquire_a_second_buffer() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let mut r = f.registry();
        symlink("one.md", f.0.join("symbolic.md")).unwrap();
        fs::hard_link(f.0.join("one.md"), f.0.join("hard.md")).unwrap();
        r.assign(Pane::Primary, Path::new("one.md")).unwrap();
        for alias in ["symbolic.md", "hard.md"] {
            assert_eq!(
                r.assign(Pane::Secondary, Path::new(alias)).unwrap(),
                Assignment::OwnedBy(Pane::Primary)
            );
            assert_eq!(r.path(Pane::Secondary), None);
        }
    }
    #[cfg(unix)]
    #[test]
    fn a_symlink_outside_the_project_is_not_editable() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let outside = Fixture::new();
        let mut r = f.registry();
        symlink(outside.0.join("one.md"), f.0.join("escape.md")).unwrap();
        assert!(r.assign(Pane::Secondary, Path::new("escape.md")).is_err());
        assert_eq!(
            fs::read_to_string(outside.0.join("one.md")).unwrap(),
            "First document\n"
        );
    }
    #[cfg(unix)]
    #[test]
    fn external_alias_creation_stops_validation_without_discarding_either_lease() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let mut r = f.registry();
        r.assign(Pane::Primary, Path::new("one.md")).unwrap();
        r.assign(Pane::Secondary, Path::new("two.md")).unwrap();
        fs::remove_file(f.0.join("two.md")).unwrap();
        symlink("one.md", f.0.join("two.md")).unwrap();
        assert!(r.validate().is_err());
        assert_eq!(r.path(Pane::Primary), Some(Path::new("one.md")));
        assert_eq!(r.path(Pane::Secondary), Some(Path::new("two.md")));
    }
    #[cfg(unix)]
    #[test]
    fn atomic_replacement_refreshes_inodes_instead_of_claiming_a_historical_hardlink() {
        let f = Fixture::new();
        let mut r = f.registry();
        fs::hard_link(f.0.join("one.md"), f.0.join("historical.md")).unwrap();
        r.assign(Pane::Primary, Path::new("one.md")).unwrap();
        fs::write(f.0.join("replacement.tmp"), "Replacement\n").unwrap();
        fs::rename(f.0.join("replacement.tmp"), f.0.join("one.md")).unwrap();
        r.validate().unwrap();
        assert_eq!(
            r.assign(Pane::Secondary, Path::new("historical.md"))
                .unwrap(),
            Assignment::Assigned
        );
    }
}
