//! Publish affected Markdown pages from a consistent SQLite read snapshot.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};

use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{Change, NodeId};
use crate::projection::{Result, op_key};
use crate::{search, storage};

pub(crate) fn create_tables(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS readable_pending(
            page TEXT PRIMARY KEY, stamp BLOB NOT NULL
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS readable_reset(
            singleton INTEGER PRIMARY KEY CHECK(singleton=1), stamp BLOB NOT NULL
         );",
    )?;
    Ok(())
}

pub(crate) fn rebuilt(db: &Connection, stamp: &[u8]) -> Result<()> {
    db.prepare_cached(
        "INSERT OR REPLACE INTO readable_pending SELECT node,?1 FROM live_outline WHERE parent=''",
    )?
    .execute([stamp])?;
    db.execute(
        "INSERT OR REPLACE INTO readable_reset VALUES (1,?1)",
        [stamp],
    )?;
    Ok(())
}

fn root_of(db: &Connection, node: &str) -> Result<Option<String>> {
    Ok(db
        .prepare_cached(
            "WITH RECURSIVE ancestors(node,parent) AS (
            SELECT node,parent FROM live_outline WHERE node=?1
            UNION ALL
            SELECT above.node,above.parent FROM live_outline AS above
            JOIN ancestors ON above.node=ancestors.parent
         ) SELECT node FROM ancestors WHERE parent=''",
        )?
        .query_row([node], |row| row.get(0))
        .optional()?)
}

pub(crate) fn affected_root(db: &Connection, change: &Change) -> Result<Option<String>> {
    let node = match change {
        Change::Create { node, .. }
        | Change::InsertText { node, .. }
        | Change::Move { node, .. }
        | Change::SetField { node, .. }
        | Change::SetTag { node, .. }
        | Change::SetLink { node, .. }
        | Change::SetSearch { node, .. } => serde_json::to_string(node)?,
        Change::PutEdition { key, .. } => serde_json::to_string(&key.work)?,
        Change::EditText { token, .. } => {
            let node = db
                .prepare_cached("SELECT node FROM live_notes WHERE token=?1")?
                .query_row([op_key(*token)], |row| row.get::<_, String>(0))
                .optional()?;
            return match node {
                Some(node) => root_of(db, &node),
                None => Ok(None),
            };
        }
        _ => return Ok(None),
    };
    root_of(db, &node)
}

pub(crate) fn mark_edit(
    db: &Connection,
    change: &Change,
    created: NodeId,
    previous: Option<String>,
    stamp: &[u8],
) -> Result<()> {
    let current = match change {
        Change::CreateWithText { .. } | Change::CreateWork { .. } => {
            root_of(db, &serde_json::to_string(&created)?)?
        }
        _ => affected_root(db, change)?,
    };
    for page in [previous, current].into_iter().flatten() {
        db.prepare_cached("INSERT OR REPLACE INTO readable_pending VALUES (?1,?2)")?
            .execute(params![page, stamp])?;
    }
    if let Change::DefineField { tag, .. } = change {
        db.prepare_cached(
            "WITH RECURSIVE branches(node,parent) AS (
               SELECT o.node,o.parent FROM live_outline AS o
               JOIN note_tags AS t ON t.node=o.node WHERE t.target=?1
               UNION
               SELECT o.node,o.parent FROM live_outline AS o JOIN branches AS b ON o.node=b.parent
             ) INSERT OR REPLACE INTO readable_pending SELECT node,?2 FROM branches WHERE parent=''",
        )?.execute(params![serde_json::to_string(tag)?, stamp])?;
    }
    let target = match change {
        Change::Move { node, .. } => Some(serde_json::to_string(node)?),
        Change::EditText { token, .. } => db
            .prepare_cached("SELECT node FROM live_notes WHERE token=?1")?
            .query_row([op_key(*token)], |row| row.get::<_, String>(0))
            .optional()?,
        _ => None,
    };
    if let Some(target) = target {
        db.prepare_cached(
            "WITH RECURSIVE moved(node) AS (
                 SELECT ?1 UNION ALL SELECT o.node FROM live_outline AS o
                 JOIN moved ON o.parent=moved.node WHERE ?2
             ), branches(node,parent) AS (
                 SELECT o.node,o.parent FROM live_outline AS o WHERE o.node IN (
                     SELECT l.node FROM note_links AS l JOIN moved ON moved.node=l.target
                     UNION SELECT t.node FROM note_tags AS t JOIN moved ON moved.node=t.target
                 )
                 UNION SELECT o.node,o.parent FROM live_outline AS o JOIN branches AS b ON o.node=b.parent
             ) INSERT OR REPLACE INTO readable_pending SELECT node,?3 FROM branches WHERE parent=''",
        )?.execute(params![target, matches!(change, Change::Move { .. }), stamp])?;
    }
    Ok(())
}

pub(crate) struct ReadableWorker {
    wake: mpsc::SyncSender<()>,
    errors: mpsc::Receiver<String>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ReadableWorker {
    pub(crate) fn start(db: PathBuf, directory: PathBuf) -> Result<Self> {
        let (wake, work) = mpsc::sync_channel(1);
        let (report, errors) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = stop.clone();
        let thread = thread::Builder::new()
            .name("Brain Markdown".into())
            .spawn(move || {
                let mut first = true;
                while work.recv().is_ok() && !stop_worker.load(Ordering::Relaxed) {
                    if let Err(error) = export(&db, &directory, &stop_worker, first) {
                        let _ = report.try_send(error.to_string());
                    } else {
                        first = false;
                    }
                }
            })?;
        Ok(Self {
            wake,
            errors,
            stop,
            thread: Some(thread),
        })
    }

    pub(crate) fn notify(&self) {
        let _ = self.wake.try_send(());
    }

    pub(crate) fn error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }
}

impl Drop for ReadableWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.notify();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn export(path: &Path, directory: &Path, stop: &AtomicBool, first: bool) -> Result<()> {
    let db = Connection::open(path)?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    db.pragma_update(None, "synchronous", "NORMAL")?;
    fs::create_dir_all(directory)?;
    if first {
        recover_copies(&db, directory, stop)?;
    }
    let reset: Option<Vec<u8>> = db
        .query_row("SELECT stamp FROM readable_reset", [], |row| row.get(0))
        .optional()?;
    if let Some(reset) = reset {
        for file in fs::read_dir(directory)? {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            let file = file?;
            let name = file.file_name();
            if let Some(id) = name
                .to_str()
                .and_then(|name| name.strip_suffix(".md"))
                .and_then(|name| name.parse::<NodeId>().ok())
                && !is_page(&db, &serde_json::to_string(&id)?)?
            {
                remove_copy(&file.path())?;
            }
        }
        db.execute("DELETE FROM readable_reset WHERE stamp=?1", [reset])?;
    }
    while !stop.load(Ordering::Relaxed) {
        let read = db.unchecked_transaction()?;
        let job: Option<(String, Vec<u8>)> = read
            .prepare_cached("SELECT page,stamp FROM readable_pending ORDER BY page LIMIT 1")?
            .query_row([], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()?;
        let Some((page, stamp)) = job else {
            let stamp: Option<Vec<u8>> = read
                .prepare_cached("SELECT stamp FROM view_state")?
                .query_row([], |row| row.get(0))
                .optional()?;
            read.commit()?;
            if let Some(stamp) = stamp {
                let version = stamp_text(&stamp);
                let file = directory.join(".version");
                if fs::read_to_string(&file).ok().as_deref() != Some(&version) {
                    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
                    temporary.write_all(version.as_bytes())?;
                    publish(temporary, &file)?;
                }
            }
            let _: (i64, i64, i64) = db.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            return Ok(());
        };
        let id: NodeId = serde_json::from_str(&page)?;
        let path = directory.join(format!("{}.md", id.external_id()));
        if is_page(&read, &page)? {
            let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
            if !render(&read, &mut temporary, id, &page, stop)? {
                return Ok(());
            }
            read.commit()?;
            publish(temporary, &path)?;
        } else {
            read.commit()?;
            remove_copy(&path)?;
        }
        db.prepare_cached("DELETE FROM readable_pending WHERE page=?1 AND stamp=?2")?
            .execute(params![page, stamp])?;
    }
    Ok(())
}

fn stamp_text(stamp: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(stamp.len() * 2);
    for byte in stamp {
        write!(text, "{byte:02x}").expect("String format");
    }
    text
}

fn recover_copies(db: &Connection, directory: &Path, stop: &AtomicBool) -> Result<()> {
    let stamp: Option<Vec<u8>> = db
        .query_row("SELECT stamp FROM view_state", [], |row| row.get(0))
        .optional()?;
    let Some(stamp) = stamp else {
        return Ok(());
    };
    let pending: bool =
        db.query_row("SELECT EXISTS(SELECT 1 FROM readable_pending)", [], |row| {
            row.get(0)
        })?;
    let version = fs::read_to_string(directory.join(".version")).ok();
    if !pending && version.as_deref() != Some(&stamp_text(&stamp)) {
        return rebuilt(db, &stamp);
    }
    // Detect a removed copy even when the last recorded version is current.
    let mut last = String::new();
    while !stop.load(Ordering::Relaxed) {
        let keys: Vec<String> = db
            .prepare_cached(
                "SELECT node FROM live_outline WHERE parent='' AND node>?1 ORDER BY node LIMIT 64",
            )?
            .query_map([&last], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        if keys.is_empty() {
            break;
        }
        for key in &keys {
            let node: NodeId = serde_json::from_str(key)?;
            if !directory
                .join(format!("{}.md", node.external_id()))
                .is_file()
            {
                db.prepare_cached("INSERT OR IGNORE INTO readable_pending VALUES (?1,?2)")?
                    .execute(params![key, stamp])?;
            }
        }
        last = keys.into_iter().last().expect("Nonempty page batch");
    }
    Ok(())
}

fn is_page(db: &Connection, key: &str) -> Result<bool> {
    Ok(db
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM live_outline WHERE node=?1 AND parent='')")?
        .query_row([key], |row| row.get(0))?)
}

fn publish(temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    storage::sync_directory(path.parent().expect("Markdown directory"))?;
    Ok(())
}

fn remove_copy(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => storage::sync_directory(path.parent().expect("Markdown directory"))?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn render(
    db: &Connection,
    output: &mut impl Write,
    root: NodeId,
    key: &str,
    stop: &AtomicBool,
) -> Result<bool> {
    let title: String = db
        .prepare_cached("SELECT body FROM live_notes WHERE node=?1")?
        .query_row([key], |row| row.get(0))?;
    let heading = match root {
        NodeId::Today(date) => date.to_string(),
        NodeId::Builtin(tag) if title.is_empty() => tag.name().into(),
        _ => title.lines().next().unwrap_or("").into(),
    };
    writeln!(
        output,
        "<a id=\"{}\"></a>\n# {}\n",
        root.external_id(),
        heading
    )?;
    write_details(db, output, root, "")?;
    let mut parents = vec![(key.to_owned(), -1)];
    while !parents.is_empty() {
        if stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let depth = parents.len() - 1;
        let (parent, offset) = &mut parents[depth];
        let child: Option<(String, String, i64)> = db
            .prepare_cached(
                "SELECT outline.node,notes.body,outline.position FROM live_outline AS outline
             JOIN live_notes AS notes ON notes.node=outline.node
             WHERE outline.parent=?1 AND outline.position>?2 ORDER BY outline.position LIMIT 1",
            )?
            .query_row(params![&*parent, *offset], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .optional()?;
        if let Some((key, text, position)) = child {
            *offset = position;
            let id: NodeId = serde_json::from_str(&key)?;
            let indent = "  ".repeat(depth);
            let mut lines = text.lines();
            writeln!(
                output,
                "{indent}- <a id=\"{}\"></a>{}",
                id.external_id(),
                lines.next().unwrap_or("")
            )?;
            for line in lines {
                writeln!(output, "{indent}  {line}")?;
            }
            write_details(db, output, id, &format!("{indent}  "))?;
            parents.push((key, -1));
        } else {
            parents.pop();
        }
    }
    Ok(true)
}

fn write_details(
    db: &Connection,
    output: &mut impl Write,
    node: NodeId,
    indent: &str,
) -> Result<()> {
    let details = search::details(db, node)?;
    let mut schema = std::collections::BTreeMap::new();
    for tag in details.tags {
        let name = match tag {
            NodeId::Builtin(tag) => tag.name().into(),
            _ => crate::projection::read_note(db, tag)?.unwrap_or_else(|| tag.external_id()),
        };
        writeln!(output, "{indent}  Tag: {name}")?;
        schema.extend(search::fields_for_tag(db, tag)?);
    }
    for (field, value) in details.fields {
        let name = schema
            .get(&field)
            .map(|spec| spec.name.clone())
            .unwrap_or(serde_json::to_string(&field)?);
        writeln!(output, "{indent}  {name}: {value}")?;
    }
    for target in details.links {
        let id = target.external_id();
        let title = crate::projection::read_note(db, target)?.unwrap_or_else(|| id.clone());
        let title = title.lines().next().unwrap_or(&id).replace(['[', ']'], "");
        if let Some(root) = root_of(db, &serde_json::to_string(&target)?)? {
            let root: NodeId = serde_json::from_str(&root)?;
            writeln!(
                output,
                "{indent}  [{title}]({}.md#{id})",
                root.external_id()
            )?;
        } else {
            writeln!(output, "{indent}  Unresolved link: {id}")?;
        }
    }
    if let Some(query) = details.query {
        writeln!(
            output,
            "{indent}  Saved search: {}",
            serde_json::to_string(&query)?
        )?;
    }
    Ok(())
}
