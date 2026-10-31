//! Per-local-user encrypted credential store.
//!
//! Layout under the user's private state directory (`root`, mode 0700):
//! `keys/google.key` (random 256-bit key, 0600) and `google/credentials.v1`
//! (ChaCha20-Poly1305, nonce || ciphertext, 0600). The ciphertext is bound to the
//! local user name, so a file copied to another account does not open.
//!
//! Honest scope: the key file lives beside the ciphertext, so this keeps tokens out
//! of manuscript backups, exports, logs and accidental copies, and detects tampering.
//! It does not defend against code running as the same user or against an attacker
//! with the unlocked disk; full-disk encryption and file permissions provide that.

use crate::Error;
use ring::aead::{Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey, CHACHA20_POLY1305};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, Zeroizing};

const MAGIC: &[u8; 4] = b"WGC1";
const MAX_FILE: u64 = 16 * 1024;
const AAD_PREFIX: &[u8] = b"wros-google-credentials-v1:";

/// A stored sign-in. Debug output is redacted; the token is zeroized on drop.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub refresh_token: Zeroizing<String>,
    pub email: Option<String>,
    pub scopes: Vec<String>,
}
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("refresh_token", &"<redacted>")
            .field("email", &self.email)
            .field("scopes", &self.scopes)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
struct Plain {
    refresh_token: String,
    email: Option<String>,
    scopes: Vec<String>,
}

pub struct CredentialStore {
    key_file: PathBuf,
    data_file: PathBuf,
    root: PathBuf,
    user: String,
}

fn bad(why: &'static str) -> Error {
    Error::Store(why)
}

impl CredentialStore {
    /// `root` is the writer's private state directory; `user` is their local
    /// account name (bound into the ciphertext).
    pub fn new(root: impl Into<PathBuf>, user: &str) -> Result<Self, Error> {
        if user.is_empty() || user.len() > 64 || !user.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(bad("invalid local user name"));
        }
        let root = root.into();
        Ok(Self {
            key_file: root.join("keys/google.key"),
            data_file: root.join("google/credentials.v1"),
            root,
            user: user.to_owned(),
        })
    }

    pub fn exists(&self) -> bool {
        fs::symlink_metadata(&self.data_file).is_ok()
    }

    /// Create `dir` (and parents under `root`) as 0700, refusing symlinks and
    /// directories that another user owns or can write.
    fn private_dir(&self, dir: &Path) -> Result<(), Error> {
        let mut path = self.root.clone();
        fs::create_dir_all(&path).map_err(|_| bad("cannot create state directory"))?;
        let relative = dir.strip_prefix(&self.root).map_err(|_| bad("path outside state root"))?;
        let mut targets = vec![path.clone()];
        for part in relative.components() {
            path.push(part);
            targets.push(path.clone());
        }
        for target in &targets {
            match fs::symlink_metadata(target) {
                Ok(_) => {}
                Err(_) => {
                    fs::create_dir(target).map_err(|_| bad("cannot create private directory"))?;
                    fs::set_permissions(target, fs::Permissions::from_mode(0o700))
                        .map_err(|_| bad("cannot restrict private directory"))?;
                }
            }
            let meta = fs::symlink_metadata(target).map_err(|_| bad("cannot inspect directory"))?;
            // SAFETY: geteuid has no preconditions.
            let uid = unsafe { libc::geteuid() };
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(bad("state path is not a plain directory"));
            }
            // Only the leaf directories we own and control must be private; the
            // caller-provided root itself may be a normal home directory.
            if target != &self.root && (meta.uid() != uid || meta.mode() & 0o077 != 0) {
                return Err(bad("private directory is shared or owned by another user"));
            }
        }
        Ok(())
    }

    fn open_regular(path: &Path) -> Result<fs::File, Error> {
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|_| bad("cannot open credential file"))?;
        let meta = file.metadata().map_err(|_| bad("cannot inspect credential file"))?;
        // SAFETY: geteuid has no preconditions.
        if !meta.is_file() || meta.len() > MAX_FILE || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Err(bad("credential file is unsafe"));
        }
        Ok(file)
    }

    fn write_private(path: &Path, data: &[u8]) -> Result<(), Error> {
        let temp = path.with_extension("tmp");
        let _ = fs::remove_file(&temp);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp)
            .map_err(|_| bad("cannot create credential file"))?;
        file.write_all(data).and_then(|_| file.sync_all()).map_err(|_| bad("cannot write credential file"))?;
        drop(file);
        fs::rename(&temp, path).map_err(|_| bad("cannot replace credential file"))?;
        if let Some(parent) = path.parent() {
            fs::File::open(parent).and_then(|d| d.sync_all()).map_err(|_| bad("cannot sync credential directory"))?;
        }
        Ok(())
    }

    fn key(&self, create: bool) -> Result<LessSafeKey, Error> {
        let mut material = Zeroizing::new([0u8; 32]);
        if fs::symlink_metadata(&self.key_file).is_err() {
            if !create {
                return Err(bad("credential key is missing"));
            }
            self.private_dir(self.key_file.parent().unwrap())?;
            SystemRandom::new().fill(&mut material[..]).map_err(|_| bad("no secure randomness"))?;
            Self::write_private(&self.key_file, &material[..])?;
        } else {
            self.private_dir(self.key_file.parent().unwrap())?;
            let mut file = Self::open_regular(&self.key_file)?;
            let mut read = Zeroizing::new(Vec::with_capacity(33));
            Read::by_ref(&mut file).take(33).read_to_end(&mut read).map_err(|_| bad("cannot read credential key"))?;
            if read.len() != 32 {
                return Err(bad("credential key is damaged"));
            }
            material.copy_from_slice(&read);
        }
        let unbound = UnboundKey::new(&CHACHA20_POLY1305, &material[..]).map_err(|_| bad("invalid credential key"))?;
        Ok(LessSafeKey::new(unbound))
    }

    fn aad(&self) -> Vec<u8> {
        [AAD_PREFIX, self.user.as_bytes()].concat()
    }

    pub fn save(&self, credentials: &Credentials) -> Result<(), Error> {
        let plain = Plain {
            refresh_token: credentials.refresh_token.to_string(),
            email: credentials.email.clone(),
            scopes: credentials.scopes.clone(),
        };
        let mut json = Zeroizing::new(serde_json::to_vec(&plain).map_err(|_| bad("cannot encode credentials"))?);
        let mut plain = plain;
        plain.refresh_token.zeroize();
        let key = self.key(true)?;
        let mut nonce = [0u8; NONCE_LEN];
        SystemRandom::new().fill(&mut nonce).map_err(|_| bad("no secure randomness"))?;
        let mut sealed = std::mem::take(&mut *json);
        key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(self.aad()), &mut sealed)
            .map_err(|_| bad("cannot encrypt credentials"))?;
        let mut file = Vec::with_capacity(MAGIC.len() + NONCE_LEN + sealed.len());
        file.extend_from_slice(MAGIC);
        file.extend_from_slice(&nonce);
        file.extend_from_slice(&sealed);
        self.private_dir(self.data_file.parent().unwrap())?;
        Self::write_private(&self.data_file, &file)
    }

    pub fn load(&self) -> Result<Option<Credentials>, Error> {
        if fs::symlink_metadata(&self.data_file).is_err() {
            return Ok(None);
        }
        self.private_dir(self.data_file.parent().unwrap())?;
        let mut raw = Vec::new();
        Self::open_regular(&self.data_file)?
            .take(MAX_FILE + 1)
            .read_to_end(&mut raw)
            .map_err(|_| bad("cannot read credential file"))?;
        if raw.len() < MAGIC.len() + NONCE_LEN + CHACHA20_POLY1305.tag_len() || &raw[..4] != MAGIC {
            return Err(bad("credential file is damaged"));
        }
        let nonce: [u8; NONCE_LEN] = raw[4..4 + NONCE_LEN].try_into().unwrap();
        let mut sealed = raw[4 + NONCE_LEN..].to_vec();
        let key = self.key(false)?;
        let opened = key
            .open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(self.aad()), &mut sealed)
            .map_err(|_| bad("credential file failed authentication"))?;
        let plain: Result<Plain, Error> = serde_json::from_slice(opened).map_err(|_| bad("credential file is damaged"));
        sealed.zeroize();
        let plain = plain?;
        Ok(Some(Credentials {
            refresh_token: Zeroizing::new(plain.refresh_token),
            email: plain.email,
            scopes: plain.scopes,
        }))
    }

    /// Remove the stored sign-in and its key (disconnect). Missing files are fine.
    pub fn delete(&self) -> Result<(), Error> {
        for path in [&self.data_file, &self.key_file] {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(bad("cannot remove credential file")),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Credentials {
        Credentials {
            refresh_token: Zeroizing::new("1//refresh-secret-value".into()),
            email: Some("writer@example.invalid".into()),
            scopes: vec!["https://www.googleapis.com/auth/drive.file".into()],
        }
    }
    fn dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!("wros-google-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn round_trips_and_keeps_the_token_out_of_the_file_and_debug_output() {
        let root = dir();
        let store = CredentialStore::new(&root, "writer").unwrap();
        assert!(store.load().unwrap().is_none());
        store.save(&sample()).unwrap();
        let raw = fs::read(root.join("google/credentials.v1")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("refresh-secret"));
        assert!(!format!("{:?}", sample()).contains("refresh-secret"));
        assert_eq!(store.load().unwrap().unwrap(), sample());
        for (path, mode) in [("google", 0o700), ("keys", 0o700), ("google/credentials.v1", 0o600), ("keys/google.key", 0o600)] {
            assert_eq!(fs::metadata(root.join(path)).unwrap().mode() & 0o777, mode, "{path}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_copied_file_does_not_open_for_another_local_user() {
        let root = dir();
        CredentialStore::new(&root, "writer").unwrap().save(&sample()).unwrap();
        let other = CredentialStore::new(&root, "intruder").unwrap();
        assert!(other.load().is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tampering_and_truncation_are_detected() {
        let root = dir();
        let store = CredentialStore::new(&root, "writer").unwrap();
        store.save(&sample()).unwrap();
        let path = root.join("google/credentials.v1");
        let mut raw = fs::read(&path).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 1;
        fs::write(&path, &raw).unwrap();
        assert!(store.load().is_err());
        fs::write(&path, &raw[..10]).unwrap();
        assert!(store.load().is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn symlinked_and_shared_paths_are_refused() {
        let root = dir();
        let store = CredentialStore::new(&root, "writer").unwrap();
        store.save(&sample()).unwrap();
        fs::set_permissions(root.join("google/credentials.v1"), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(store.load().is_err(), "group/world readable file");
        fs::set_permissions(root.join("google/credentials.v1"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(root.join("keys"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(store.load().is_err(), "shared key directory");
        fs::set_permissions(root.join("keys"), fs::Permissions::from_mode(0o700)).unwrap();
        let real = root.join("google/credentials.v1");
        let moved = root.join("elsewhere");
        fs::rename(&real, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &real).unwrap();
        assert!(store.load().is_err(), "symlinked credential file");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn disconnect_removes_both_files_and_is_idempotent() {
        let root = dir();
        let store = CredentialStore::new(&root, "writer").unwrap();
        store.save(&sample()).unwrap();
        store.delete().unwrap();
        store.delete().unwrap();
        assert!(!store.exists() && store.load().unwrap().is_none());
        assert!(!root.join("keys/google.key").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn overwriting_keeps_one_valid_file_and_rejects_bad_user_names() {
        let root = dir();
        let store = CredentialStore::new(&root, "writer").unwrap();
        store.save(&sample()).unwrap();
        let mut newer = sample();
        newer.refresh_token = Zeroizing::new("1//second".into());
        store.save(&newer).unwrap();
        assert_eq!(store.load().unwrap().unwrap(), newer);
        assert!(!root.join("google/credentials.v1.tmp").exists());
        assert!(CredentialStore::new(&root, "").is_err());
        assert!(CredentialStore::new(&root, "bad name").is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
