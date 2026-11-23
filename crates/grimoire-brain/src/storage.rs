//! Local journal and checkpoint recovery for the Brain storage tests.
//!
//! Each acknowledged update has its own checked file. Publish a checkpoint
//! only after synchronizing its files. Keep enough journal to reopen the
//! previous checkpoint, and retain original updates while any are pending.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::loro_store::{Checkpoint, LoroStore, StoreError};
use crate::model::{Edit, Snapshot};

const CURRENT: &str = "CURRENT";
const JOURNAL: &str = "journal";
const CHECKSUM_LEN: usize = 32;
const MANIFEST_SCHEMA: u32 = 1;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Another process already has the Brain open for writing")]
    WriterBusy,
    #[error("The last save could not be confirmed. Reopen the Brain before editing.")]
    ReopenRequired,
    #[error("Invalid Brain checkpoint manifest")]
    InvalidManifest,
    #[error("Brain storage I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("Invalid Brain checkpoint metadata: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub used_previous_checkpoint: bool,
    pub quarantined: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
struct Slot {
    generation: u64,
    covered_through: u64,
    has_pending: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
struct Manifest {
    schema: u32,
    active: Slot,
    previous: Option<Slot>,
}

pub struct DurableStore {
    root: PathBuf,
    _writer: File,
    store: LoroStore,
    selected: Option<Slot>,
    generation: u64,
    journal_tail: u64,
    recovery: RecoveryReport,
    failed: bool,
}

impl DurableStore {
    pub fn open(root: &Path, device: u64) -> Result<Self, StorageError> {
        fs::create_dir_all(root)?;
        if let Some(parent) = root.parent().filter(|path| !path.as_os_str().is_empty()) {
            sync_directory(parent)?;
        }
        let writer = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("writer.lock"))?;
        match writer.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(StorageError::WriterBusy),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        let journal = root.join(JOURNAL);
        fs::create_dir_all(&journal)?;
        sync_directory(root)?;

        let manifest = read_manifest(root)?;
        let generation = manifest.map_or(0, |value| value.active.generation);
        let mut recovery = RecoveryReport::default();
        let (mut store, selected) = match manifest {
            None => (LoroStore::new(device)?, None),
            Some(manifest) => match open_slot(root, manifest.active, device) {
                Ok(store) => (store, Some(manifest.active)),
                Err(error) => {
                    let Some(previous) = manifest.previous else {
                        return Err(error);
                    };
                    let store = open_slot(root, previous, device)?;
                    recovery.used_previous_checkpoint = true;
                    (store, Some(previous))
                }
            },
        };

        quarantine_partial_files(root, &journal, generation, &mut recovery)?;
        let covered = selected.map_or(0, |previous| previous.covered_through);
        let mut journal_tail = manifest.map_or(0, |value| value.active.covered_through);
        for (number, path) in numbered_files(&journal, "", ".update")? {
            journal_tail = journal_tail.max(number);
            if number <= covered {
                continue;
            }
            match read_checked(&path) {
                Ok(bytes) => store.import(&bytes)?,
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                    quarantine(root, &path, &mut recovery)?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(Self {
            root: root.to_owned(),
            _writer: writer,
            store,
            selected,
            generation,
            journal_tail,
            recovery,
            failed: false,
        })
    }

    pub fn recovery(&self) -> &RecoveryReport {
        &self.recovery
    }

    pub fn project(&self) -> Result<Snapshot, StorageError> {
        self.require_healthy()?;
        Ok(self.store.project()?)
    }

    pub fn has_pending(&self) -> Result<bool, StorageError> {
        self.require_healthy()?;
        Ok(self.store.has_pending()?)
    }

    pub fn append(&mut self, edit: Edit) -> Result<Vec<u8>, StorageError> {
        self.require_healthy()?;
        let bytes = match self.store.append(edit) {
            Ok(bytes) => bytes,
            Err(error) => {
                // A failed export can happen after Loro applied the edit.
                self.failed |= matches!(error, StoreError::Encode(_) | StoreError::Loro(_));
                return Err(error.into());
            }
        };
        if !bytes.is_empty() {
            self.save_update(&bytes)?;
        }
        Ok(bytes)
    }

    pub fn import(&mut self, bytes: &[u8]) -> Result<(), StorageError> {
        self.require_healthy()?;
        self.store.import(bytes)?;
        self.save_update(bytes)
    }

    pub fn checkpoint(&mut self) -> Result<(), StorageError> {
        self.require_healthy()?;
        let checkpoint = self.store.checkpoint()?;
        let generation = next_number(self.generation)?;
        let previous = self.selected;
        let active = Slot {
            generation,
            covered_through: if checkpoint.pending.is_none() {
                self.journal_tail
            } else {
                previous.map_or(0, |slot| slot.covered_through)
            },
            has_pending: checkpoint.pending.is_some(),
        };
        if let Err(error) = publish_checkpoint(&self.root, active, previous, &checkpoint) {
            self.failed = true;
            return Err(error);
        }
        self.generation = generation;
        self.selected = Some(active);
        prune_before(&self.root, previous)?;
        Ok(())
    }

    fn save_update(&mut self, bytes: &[u8]) -> Result<(), StorageError> {
        let saved = next_number(self.journal_tail).and_then(|number| {
            write_checked(
                &self.root.join(JOURNAL).join(format!("{number:020}.update")),
                bytes,
            )?;
            Ok(number)
        });
        match saved {
            Ok(number) => self.journal_tail = number,
            Err(error) => {
                self.failed = true;
                return Err(error.into());
            }
        }
        Ok(())
    }

    fn require_healthy(&self) -> Result<(), StorageError> {
        if self.failed {
            return Err(StorageError::ReopenRequired);
        }
        Ok(())
    }
}

fn read_manifest(root: &Path) -> Result<Option<Manifest>, StorageError> {
    let bytes = match read_checked(&root.join(CURRENT)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    if manifest.schema != MANIFEST_SCHEMA
        || manifest.active.generation == 0
        || manifest.previous.is_some_and(|previous| {
            previous.generation == 0
                || previous.generation >= manifest.active.generation
                || previous.covered_through > manifest.active.covered_through
        })
    {
        return Err(StorageError::InvalidManifest);
    }
    Ok(Some(manifest))
}

fn open_slot(root: &Path, slot: Slot, device: u64) -> Result<LoroStore, StorageError> {
    let snapshot = read_checked(&checkpoint_file(root, slot.generation, "snapshot"))?;
    let pending = slot
        .has_pending
        .then(|| read_checked(&checkpoint_file(root, slot.generation, "pending")))
        .transpose()?;
    Ok(LoroStore::restore(
        device,
        &Checkpoint { snapshot, pending },
    )?)
}

fn publish_checkpoint(
    root: &Path,
    active: Slot,
    previous: Option<Slot>,
    checkpoint: &Checkpoint,
) -> Result<(), StorageError> {
    let snapshot = checkpoint_file(root, active.generation, "snapshot");
    write_checked(&snapshot, &checkpoint.snapshot)?;
    verify_checked(&snapshot)?;
    if let Some(bytes) = &checkpoint.pending {
        let pending = checkpoint_file(root, active.generation, "pending");
        write_checked(&pending, bytes)?;
        verify_checked(&pending)?;
    }
    let manifest = Manifest {
        schema: MANIFEST_SCHEMA,
        active,
        previous,
    };
    write_checked(&root.join(CURRENT), &serde_json::to_vec(&manifest)?)?;
    Ok(())
}

fn checkpoint_file(root: &Path, generation: u64, kind: &str) -> PathBuf {
    root.join(format!("checkpoint-{generation:020}.{kind}"))
}

fn prune_before(root: &Path, previous: Option<Slot>) -> io::Result<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let journal = root.join(JOURNAL);
    for (number, path) in numbered_files(&journal, "", ".update")? {
        if number <= previous.covered_through {
            fs::remove_file(path)?;
        }
    }
    sync_directory(&journal)?;
    for suffix in [".snapshot", ".pending"] {
        for (number, path) in numbered_files(root, "checkpoint-", suffix)? {
            if number < previous.generation {
                fs::remove_file(path)?;
            }
        }
    }
    sync_directory(root)
}

fn quarantine_partial_files(
    root: &Path,
    journal: &Path,
    generation: u64,
    report: &mut RecoveryReport,
) -> io::Result<()> {
    let manifest = root.join("CURRENT.new");
    if manifest.is_file() {
        quarantine(root, &manifest, report)?;
    }
    for (directory, prefix) in [(root, "checkpoint-"), (journal, "")] {
        for (_, path) in numbered_files(directory, prefix, ".new")? {
            quarantine(root, &path, report)?;
        }
    }
    for suffix in [".snapshot", ".pending"] {
        for (number, path) in numbered_files(root, "checkpoint-", suffix)? {
            if number > generation {
                quarantine(root, &path, report)?;
            }
        }
    }
    Ok(())
}

fn quarantine(root: &Path, path: &Path, report: &mut RecoveryReport) -> io::Result<()> {
    let destination = root.join("quarantine");
    fs::create_dir_all(&destination)?;
    let filename = path
        .file_name()
        .expect("generated filename")
        .to_string_lossy();
    let mut target = destination.join(filename.as_ref());
    let mut suffix = 1;
    while target.exists() {
        target = destination.join(format!("{filename}.{suffix}"));
        suffix = next_number(suffix)?;
    }
    fs::rename(path, &target)?;
    sync_directory(path.parent().expect("generated path has a parent"))?;
    sync_directory(&destination)?;
    sync_directory(root)?;
    report.quarantined.push(target);
    Ok(())
}

fn numbered_files(directory: &Path, prefix: &str, suffix: &str) -> io::Result<Vec<(u64, PathBuf)>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(digits) = name
            .to_str()
            .and_then(|name| name.strip_prefix(prefix))
            .and_then(|name| name.strip_suffix(suffix))
        else {
            continue;
        };
        if digits.len() == 20
            && digits.bytes().all(|digit| digit.is_ascii_digit())
            && let Ok(number) = digits.parse::<u64>()
        {
            files.push((number, entry.path()));
        }
    }
    files.sort_unstable_by_key(|(number, _)| *number);
    Ok(files)
}

fn next_number(number: u64) -> io::Result<u64> {
    number.checked_add(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Brain storage sequence exhausted",
        )
    })
}

fn write_checked(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("new");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&Sha256::digest(bytes))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)?;
    sync_directory(path.parent().expect("generated path has a parent"))
}

fn checked_reader(path: &Path) -> io::Result<(File, [u8; CHECKSUM_LEN])> {
    let mut file = File::open(path)?;
    let mut expected = [0; CHECKSUM_LEN];
    file.read_exact(&mut expected).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            checksum_error(path)
        } else {
            error
        }
    })?;
    Ok((file, expected))
}

fn read_checked(path: &Path) -> io::Result<Vec<u8>> {
    let (mut file, expected) = checked_reader(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if <[u8; CHECKSUM_LEN]>::from(Sha256::digest(&bytes)) != expected {
        return Err(checksum_error(path));
    }
    Ok(bytes)
}

fn verify_checked(path: &Path) -> io::Result<()> {
    let (mut file, expected) = checked_reader(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if <[u8; CHECKSUM_LEN]>::from(digest.finalize()) != expected {
        return Err(checksum_error(path));
    }
    Ok(())
}

fn checksum_error(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("Brain checksum mismatch in {}", path.display()),
    )
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    // The Stage 0 durability gate runs on WrOS. Rust does not provide a
    // portable directory synchronization operation on the other targets.
    Ok(())
}
