//! Bounded guarded metadata, never prose storage. No locks/CAS are claimed.
use crate::sync::{Fingerprint, SaveOutcome};
use anyhow::{Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
pub const MAX_BYTES: usize = 1_048_576;
pub trait Validate {
    fn validate(&self) -> Result<()>;
    /// Optional cross-file invariants, checked before allocating identities or
    /// publishing an update. This is a guard, not a cross-process transaction.
    fn validate_context(&self, _root: &Path) -> Result<()> {
        Ok(())
    }
}
/// Metadata aliases and special files are refused before opening. Existing
/// ancestors must be ordinary directories; absence is not a write request.
pub fn checked(root: &Path, relative: &Path) -> Result<PathBuf> {
    crate::workspaces::relative(relative)?;
    if !relative.starts_with(".grimoire/writingdeck") {
        bail!("not WritingDeck metadata");
    }
    let root = fs::canonicalize(root)?;
    let path = root.join(relative);
    let mut at = root.clone();
    for component in relative.components() {
        at.push(component.as_os_str());
        match fs::symlink_metadata(&at) {
            Ok(m) => {
                if m.file_type().is_symlink()
                    || (!m.is_dir() && at != path)
                    || (at == path && !m.is_file())
                {
                    bail!("metadata alias/special path refused; original preserved");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}
pub fn read(root: &Path, relative: &Path) -> Result<Option<Vec<u8>>> {
    let path = checked(root, relative)?;
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        bail!("metadata exceeds 1 MiB; original preserved");
    }
    Ok(Some(bytes))
}
pub struct Store<T> {
    root: PathBuf,
    relative: PathBuf,
    pub value: T,
    seen: Option<Fingerprint>,
}
impl<T: Validate + Serialize + DeserializeOwned + Clone> Store<T> {
    pub fn load(root: &Path, relative: &Path, default: T) -> Result<Self> {
        let bytes = read(root, relative)?;
        let seen = bytes.as_ref().map(|v| Fingerprint::of_bytes(v));
        let value = match bytes {
            Some(b) => serde_json::from_slice(&b)?,
            None => default,
        };
        value.validate()?;
        value.validate_context(root)?;
        Ok(Self {
            root: fs::canonicalize(root)?,
            relative: relative.into(),
            value,
            seen,
        })
    }
    pub fn exists(&self) -> bool {
        self.seen.is_some()
    }
    pub fn verify(&self) -> Result<()> {
        if read(&self.root, &self.relative)?
            .as_ref()
            .map(|v| Fingerprint::of_bytes(v))
            != self.seen
        {
            bail!("metadata changed externally; original preserved; explicit reload required");
        }
        Ok(())
    }
    pub fn update<R>(&mut self, f: impl FnOnce(&mut T) -> Result<R>) -> Result<R> {
        self.value.validate_context(&self.root)?;
        let mut next = self.value.clone();
        let result = f(&mut next)?;
        next.validate()?;
        next.validate_context(&self.root)?;
        let text = serde_json::to_string_pretty(&next)? + "\n";
        if text.len() > MAX_BYTES {
            bail!("metadata exceeds 1 MiB; original preserved");
        }
        self.verify()?;
        let path = checked(&self.root, &self.relative)?;
        fs::create_dir_all(path.parent().unwrap())?;
        match crate::sync::save_guarded(&path, &text, self.seen)? {
            SaveOutcome::Written => {
                self.seen = Some(Fingerprint::of(&text));
                self.value = next;
                Ok(result)
            }
            SaveOutcome::Conflict { .. } => {
                bail!("metadata changed externally; no overwrite; reload explicitly")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Value {
        version: u32,
        n: u64,
    }
    impl Validate for Value {
        fn validate(&self) -> Result<()> {
            if self.version != 1 {
                bail!("future state");
            }
            Ok(())
        }
    }
    fn fixture(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("wd-state-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn unknown_future_and_oversize_bytes_survive_read_and_noop() {
        let r = fixture("unknown");
        let p = Path::new(".grimoire/writingdeck/test.json");
        let full = r.join(p);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        for bytes in [
            br#"{"version":2,"n":0}"#.to_vec(),
            br#"{"version":1,"n":0,"future":true}"#.to_vec(),
            vec![b'x'; MAX_BYTES + 2],
        ] {
            fs::write(&full, &bytes).unwrap();
            assert!(Store::load(&r, p, Value { version: 1, n: 0 }).is_err());
            assert_eq!(fs::read(&full).unwrap(), bytes);
        }
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn failed_validation_and_conflict_do_not_publish_a_clone() {
        let r = fixture("guard");
        let p = Path::new(".grimoire/writingdeck/test.json");
        let mut s = Store::load(&r, p, Value { version: 1, n: 0 }).unwrap();
        s.update(|v| {
            v.n = 3;
            Ok(())
        })
        .unwrap();
        assert!(
            s.update(|v| {
                v.version = 4;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(s.value.n, 3);
        let bytes = br#"{"version":1,"n":8}"#;
        fs::write(r.join(p), bytes).unwrap();
        assert!(
            s.update(|v| {
                v.n = 9;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(s.value.n, 3);
        assert_eq!(fs::read(r.join(p)).unwrap(), bytes);
        fs::remove_dir_all(r).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn metadata_links_and_directory_targets_are_never_opened() {
        use std::os::unix::fs::symlink;
        let r = fixture("alias");
        fs::create_dir_all(r.join("notes")).unwrap();
        symlink(r.join("notes"), r.join(".grimoire")).unwrap();
        assert!(checked(&r, Path::new(".grimoire/writingdeck/test.json")).is_err());
        fs::remove_file(r.join(".grimoire")).unwrap();
        fs::create_dir_all(r.join(".grimoire/writingdeck/test.json")).unwrap();
        assert!(read(&r, Path::new(".grimoire/writingdeck/test.json")).is_err());
        fs::remove_dir_all(r).unwrap();
    }
}
