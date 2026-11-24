//! Disk-backed candidates for the HP Stream qualification.
//!
//! The SQLite projection can be discarded and replayed from either authority.
//! Fast text replacement requires a known single token and causal winner.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Transaction, backup::Backup, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::loro_store::PagedLoroStore;
use crate::model::{Change, Edit, Model, ModelError, NodeId, OpId, Snapshot};
use crate::replica::StoreError;
use crate::storage::Journal;

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const LIVE_DB: &str = "live.sqlite";
const LEDGER_DB: &str = "ledger.sqlite";
const BACKUPS: &str = "BACKUPS.json";

pub struct PagedBrain {
    journal: Journal<PagedLoroStore>,
    view: Connection,
    root: PathBuf,
}

impl PagedBrain {
    pub fn open(root: &Path, device: u64) -> Result<Self> {
        let journal = Journal::open(root, device)?;
        let view = open_database(&root.join(LIVE_DB))?;
        create_view(&view)?;
        Ok(Self {
            journal,
            view,
            root: root.to_owned(),
        })
    }

    pub fn project(&self) -> Result<Snapshot> {
        Ok(self.journal.project()?)
    }

    pub(crate) fn prime_view(&mut self, snapshot: &Snapshot) -> Result<()> {
        replace_view(
            &mut self.view,
            &self.journal.replica().projection_stamp(),
            snapshot,
        )
    }

    pub fn note(&mut self, node: NodeId) -> Result<Option<String>> {
        self.journal.require_healthy()?;
        if !has_stamp(&self.view, &self.journal.replica().projection_stamp())? {
            self.prime_view(&self.project()?)?;
        }
        read_note(&self.view, node)
    }

    pub fn append(&mut self, edit: Edit) -> Result<Vec<u8>> {
        let before = self.journal.replica().projection_stamp();
        let update = self.journal.append(edit.clone())?;
        if !update.is_empty() {
            let transaction = self.view.transaction()?;
            if has_stamp(&transaction, &before)? && apply_text(&transaction, &edit)? {
                stamp_view(&transaction, &self.journal.replica().projection_stamp())?;
                transaction.commit()?;
            }
        }
        Ok(update)
    }

    pub fn import(&mut self, update: &[u8]) -> Result<()> {
        self.journal.import(update)?;
        Ok(())
    }

    pub fn checkpoint(&mut self) -> Result<()> {
        self.journal.checkpoint()?;
        flush_wal(&self.view)
    }

    pub(crate) fn wal_path(&self) -> PathBuf {
        self.root.join("live.sqlite-wal")
    }
}

pub struct SqliteLedger {
    root: PathBuf,
    _writer: File,
    device: u64,
    database: Connection,
}

impl SqliteLedger {
    pub fn open(root: &Path, device: u64) -> Result<Self> {
        fs::create_dir_all(root)?;
        let writer = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("writer.lock"))?;
        writer.try_lock()?;
        let database = open_database(&root.join(LEDGER_DB))?;
        database.execute_batch(
            "CREATE TABLE IF NOT EXISTS operations(
                id BLOB PRIMARY KEY, record BLOB NOT NULL
             ) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS reviews(
                id BLOB PRIMARY KEY, record TEXT NOT NULL
             ) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS ledger_state(
                singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO ledger_state VALUES (1, 0);
             CREATE TABLE IF NOT EXISTS import_state(
                singleton INTEGER PRIMARY KEY CHECK(singleton=1)
             );",
        )?;
        create_view(&database)?;
        File::open(root)?.sync_all()?;
        Ok(Self {
            root: root.to_owned(),
            _writer: writer,
            device,
            database,
        })
    }

    pub fn project(&self) -> Result<Snapshot> {
        let mut model = Model::default();
        let mut query = self
            .database
            .prepare("SELECT id, record FROM operations ORDER BY id")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let id = row.get::<_, Vec<u8>>(0)?;
            let record = row.get::<_, Vec<u8>>(1)?;
            let edit: Edit = serde_json::from_slice(&record)?;
            if id != op_key(edit.id) {
                return Err(StoreError::InvalidDocument.into());
            }
            model.insert(edit).map_err(StoreError::from)?;
        }
        Ok(model.snapshot())
    }

    pub fn edit_count(&self) -> Result<usize> {
        let count: i64 = self
            .database
            .query_row("SELECT count(*) FROM operations", [], |row| row.get(0))?;
        Ok(usize::try_from(count)?)
    }

    pub(crate) fn prime_view(&mut self, snapshot: &Snapshot) -> Result<()> {
        let revision = ledger_revision(&self.database)?;
        replace_view(&mut self.database, &revision.to_be_bytes(), snapshot)
    }

    pub fn note(&mut self, node: NodeId) -> Result<Option<String>> {
        let revision = ledger_revision(&self.database)?;
        if !has_stamp(&self.database, &revision.to_be_bytes())? {
            self.prime_view(&self.project()?)?;
        }
        read_note(&self.database, node)
    }

    pub fn append(&mut self, edit: Edit) -> Result<Vec<u8>> {
        edit.validate().map_err(StoreError::from)?;
        if edit.id.device != self.device {
            return Err(StoreError::WrongDevice(edit.id).into());
        }
        let pending =
            if self
                .database
                .query_row("SELECT EXISTS(SELECT 1 FROM import_state)", [], |row| {
                    row.get::<_, bool>(0)
                })?
            {
                Some(self.project()?.pending().clone())
            } else {
                None
            };
        for &dependency in &edit.dependencies {
            if pending
                .as_ref()
                .is_some_and(|pending| pending.contains(&dependency))
            {
                return Err(StoreError::MissingDependency(dependency).into());
            }
            if !self.database.query_row(
                "SELECT EXISTS(SELECT 1 FROM operations WHERE id=?1)",
                [op_key(dependency)],
                |row| row.get::<_, bool>(0),
            )? {
                return Err(StoreError::MissingDependency(dependency).into());
            }
        }
        let update = serde_json::to_vec(&edit)?;
        if self.save(
            &edit,
            &update,
            Some(pending.is_none_or(|pending| pending.is_empty())),
        )? {
            Ok(update)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn import(&mut self, bytes: &[u8]) -> Result<()> {
        let edit: Edit = serde_json::from_slice(bytes)?;
        edit.validate().map_err(StoreError::from)?;
        // Canonical bytes make an identical record idempotent even when its
        // transport used different whitespace.
        self.save(&edit, &serde_json::to_vec(&edit)?, None)?;
        Ok(())
    }

    fn save(
        &mut self,
        edit: &Edit,
        bytes: &[u8],
        all_dependencies_ready: Option<bool>,
    ) -> Result<bool> {
        let transaction = self.database.transaction()?;
        let revision = ledger_revision(&transaction)?;
        let mut lookup = transaction.prepare_cached("SELECT record FROM operations WHERE id=?1")?;
        let existing: Option<Vec<u8>> = lookup
            .query_row([op_key(edit.id)], |row| row.get(0))
            .optional()?;
        drop(lookup);
        if let Some(existing) = existing {
            return if existing == bytes {
                Ok(false)
            } else {
                Err(StoreError::from(ModelError::ReusedId).into())
            };
        }
        insert_record(&transaction, edit, bytes)?;
        let next = revision
            .checked_add(1)
            .ok_or("Brain ledger revision overflow")?;
        match all_dependencies_ready {
            Some(true) => {
                transaction.execute("DELETE FROM import_state", [])?;
                if has_stamp(&transaction, &revision.to_be_bytes())?
                    && apply_text(&transaction, edit)?
                {
                    stamp_view(&transaction, &next.to_be_bytes())?;
                }
            }
            Some(false) => {}
            None => {
                transaction.execute("INSERT OR IGNORE INTO import_state VALUES (1)", [])?;
            }
        }
        transaction.execute("UPDATE ledger_state SET revision=?1", [next])?;
        transaction.commit()?;
        Ok(true)
    }

    pub(crate) fn seed_transaction(&mut self) -> Result<Transaction<'_>> {
        Ok(self.database.transaction()?)
    }

    pub(crate) fn seed_edit(db: &Connection, edit: &Edit) -> Result<()> {
        insert_record(db, edit, &serde_json::to_vec(edit)?)
    }

    pub(crate) fn seed_review(db: &Connection, id: u64, record: &str) -> Result<()> {
        db.prepare_cached("INSERT INTO reviews VALUES (?1, ?2)")?
            .execute(params![id.to_be_bytes(), record])?;
        Ok(())
    }

    pub(crate) fn wal_path(&self) -> PathBuf {
        self.root.join("ledger.sqlite-wal")
    }

    pub fn checkpoint(&mut self) -> Result<()> {
        write_backup(&self.root, &self.database)?;
        flush_wal(&self.database)
    }

    /// Restore into a fresh directory and pick a new device identity.
    pub fn restore_backup(source: &Path, target: &Path, device: u64) -> Result<Self> {
        let manifest = read_backups(source)?.ok_or("No Brain ledger backup")?;
        let mut candidates = [Some(manifest.active), manifest.previous]
            .into_iter()
            .flatten();
        let backup = candidates
            .find_map(|entry| {
                let path = backup_path(source, entry.generation);
                (hash_file(&path).ok()? == entry.hash).then_some(path)
            })
            .ok_or("No verified Brain ledger backup")?;
        fs::create_dir(target)?;
        let mut copy = tempfile::NamedTempFile::new_in(target)?;
        io::copy(&mut File::open(backup)?, copy.as_file_mut())?;
        copy.as_file().sync_all()?;
        copy.persist_noclobber(target.join(LEDGER_DB))?;
        Self::open(target, device)
    }
}

fn open_database(path: &Path) -> Result<Connection> {
    let database = Connection::open(path)?;
    database.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=FULL;
         PRAGMA wal_autocheckpoint=0;
         PRAGMA cache_size=-8192;
         PRAGMA temp_store=FILE;",
    )?;
    database.busy_timeout(Duration::from_secs(2))?;
    Ok(database)
}

fn create_view(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS live_notes(
            node TEXT PRIMARY KEY, body TEXT NOT NULL, token BLOB UNIQUE, tip BLOB
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS view_state(
            singleton INTEGER PRIMARY KEY CHECK(singleton=1), stamp BLOB NOT NULL
         );",
    )?;
    Ok(())
}

fn read_note(db: &Connection, node: NodeId) -> Result<Option<String>> {
    Ok(db
        .prepare_cached("SELECT body FROM live_notes WHERE node=?1")?
        .query_row([serde_json::to_string(&node)?], |row| row.get(0))
        .optional()?)
}

fn has_stamp(db: &Connection, expected: &[u8]) -> Result<bool> {
    let stamp: Option<Vec<u8>> = db
        .prepare_cached("SELECT stamp FROM view_state WHERE singleton=1")?
        .query_row([], |row| row.get(0))
        .optional()?;
    Ok(stamp.as_deref() == Some(expected))
}

fn stamp_view(db: &Connection, stamp: &[u8]) -> Result<()> {
    db.prepare_cached("INSERT OR REPLACE INTO view_state VALUES (1, ?1)")?
        .execute([stamp])?;
    Ok(())
}

fn replace_view(db: &mut Connection, stamp: &[u8], snapshot: &Snapshot) -> Result<()> {
    let transaction = db.transaction()?;
    transaction.execute("DELETE FROM live_notes", [])?;
    let mut insert = transaction
        .prepare("INSERT INTO live_notes(node, body, token, tip) VALUES (?1, ?2, ?3, ?4)")?;
    let mut nodes = snapshot.children(None);
    while let Some(node) = nodes.pop() {
        let body = snapshot
            .body(node)
            .ok_or("Visible Brain note has no body")?;
        let winner = snapshot.single_text_winner(node);
        insert.execute(params![
            serde_json::to_string(&node)?,
            body,
            winner.map(|(token, _)| op_key(token)),
            winner.map(|(_, tip)| op_key(tip)),
        ])?;
        nodes.extend(snapshot.children(Some(node)));
    }
    drop(insert);
    stamp_view(&transaction, stamp)?;
    transaction.commit()?;
    flush_wal(db)
}

fn apply_text(db: &Connection, edit: &Edit) -> Result<bool> {
    let Change::EditText { token, text } = &edit.change else {
        return Ok(false);
    };
    let token = op_key(*token);
    let tip: Option<Vec<u8>> = db
        .prepare_cached("SELECT tip FROM live_notes WHERE token=?1")?
        .query_row([token], |row| row.get(0))
        .optional()?;
    let known_tip = tip.is_some_and(|tip| edit.dependencies.iter().any(|id| tip == op_key(*id)));
    if !known_tip {
        return Ok(false);
    }
    db.prepare_cached("UPDATE live_notes SET body=?1, tip=?2 WHERE token=?3")?
        .execute(params![text, op_key(edit.id), token])?;
    Ok(true)
}

fn ledger_revision(db: &Connection) -> Result<i64> {
    Ok(db
        .prepare_cached("SELECT revision FROM ledger_state WHERE singleton=1")?
        .query_row([], |row| row.get(0))?)
}

fn insert_record(db: &Connection, edit: &Edit, record: &[u8]) -> Result<()> {
    db.prepare_cached("INSERT INTO operations VALUES (?1, ?2)")?
        .execute(params![op_key(edit.id), record])?;
    Ok(())
}

fn op_key(id: OpId) -> [u8; 16] {
    let mut key = [0; 16];
    key[..8].copy_from_slice(&id.clock.to_be_bytes());
    key[8..].copy_from_slice(&id.device.to_be_bytes());
    key
}

fn flush_wal(db: &Connection) -> Result<()> {
    let busy = db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        row.get::<_, i64>(0)
    })?;
    if busy != 0 {
        return Err("The Brain database was busy during checkpoint".into());
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
struct BackupEntry {
    generation: u64,
    hash: [u8; 32],
}

#[derive(Deserialize, Serialize)]
struct BackupManifest {
    active: BackupEntry,
    previous: Option<BackupEntry>,
}

fn read_backups(root: &Path) -> Result<Option<BackupManifest>> {
    match fs::read(root.join(BACKUPS)) {
        Ok(data) => Ok(Some(serde_json::from_slice(&data)?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn backup_path(root: &Path, generation: u64) -> PathBuf {
    root.join(format!("ledger-{generation:020}.snapshot"))
}

fn write_backup(root: &Path, database: &Connection) -> Result<()> {
    let existing = read_backups(root)?;
    let number = existing
        .as_ref()
        .map_or(0, |manifest| manifest.active.generation)
        .checked_add(1)
        .ok_or("Brain backup generation overflow")?;
    let saved = tempfile::NamedTempFile::new_in(root)?;
    {
        let mut destination = Connection::open(saved.path())?;
        destination.pragma_update(None, "cache_size", -8192)?;
        Backup::new(database, &mut destination)?.run_to_completion(128, Duration::ZERO, None)?;
        let check: String = destination.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if check != "ok" {
            return Err("Brain ledger backup failed SQLite validation".into());
        }
    }
    saved.as_file().sync_all()?;
    let hash = hash_file(saved.path())?;
    // If a crash left this unpublished generation behind, replace it.
    saved.persist(backup_path(root, number))?;
    File::open(root)?.sync_all()?;
    let (previous, retired) = existing.map_or((None, None), |manifest| {
        (Some(manifest.active), manifest.previous)
    });
    let next = BackupManifest {
        active: BackupEntry {
            generation: number,
            hash,
        },
        previous,
    };
    let mut manifest = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer(&mut manifest, &next)?;
    manifest.write_all(b"\n")?;
    manifest.as_file().sync_all()?;
    manifest.persist(root.join(BACKUPS))?;
    File::open(root)?.sync_all()?;
    if let Some(retired) = retired {
        fs::remove_file(backup_path(root, retired.generation))?;
    }
    Ok(())
}

fn hash_file(path: &Path) -> io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hasher.update(&bytes[..count]);
    }
    Ok(hasher.finalize().into())
}
