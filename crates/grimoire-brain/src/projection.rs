//! Disposable SQLite projections for Brain. The authoritative journal and
//! checked checkpoints are sufficient to rebuild on another device.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{Change, Edit, EditFrontier, NodeId, OpId, Place, Snapshot};

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineRow {
    pub id: NodeId,
    pub body: String,
    pub has_children: bool,
}

pub(crate) fn open_database(path: &Path) -> Result<Connection> {
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

pub(crate) fn create_view(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS live_notes(
            node TEXT PRIMARY KEY NOT NULL, body TEXT NOT NULL, token BLOB UNIQUE, tip BLOB
         );
         CREATE TABLE IF NOT EXISTS view_state(
            singleton INTEGER PRIMARY KEY CHECK(singleton=1), stamp BLOB NOT NULL
         );
         CREATE TABLE IF NOT EXISTS known_nodes(node TEXT PRIMARY KEY) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS live_outline(
            node TEXT PRIMARY KEY NOT NULL, parent TEXT NOT NULL,
            position INTEGER NOT NULL, blank INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS live_outline_parent ON live_outline(parent, position);
         CREATE TABLE IF NOT EXISTS view_frontier(
            singleton INTEGER PRIMARY KEY CHECK(singleton=1), state TEXT NOT NULL
         );",
    )?;
    Ok(())
}

pub(crate) fn read_note(db: &Connection, node: NodeId) -> Result<Option<String>> {
    Ok(db
        .prepare_cached("SELECT body FROM live_notes WHERE node=?1")?
        .query_row([serde_json::to_string(&node)?], |row| row.get(0))
        .optional()?)
}

pub(crate) fn read_children(
    db: &Connection,
    parent: Option<NodeId>,
    offset: u32,
    count: u16,
    named_pages_only: bool,
) -> Result<Vec<OutlineRow>> {
    let mut query = db.prepare_cached(
        "SELECT child.node, note.body,
                EXISTS(SELECT 1 FROM live_outline WHERE parent = child.node)
         FROM live_outline AS child JOIN live_notes AS note ON note.node=child.node
         WHERE child.parent=?1 AND (NOT ?4 OR json_type(child.node, '$.Note') IS NOT NULL)
         ORDER BY child.position LIMIT ?2 OFFSET ?3",
    )?;
    let mut rows = query.query(params![
        parent_key(parent)?,
        count.min(120),
        offset,
        named_pages_only
    ])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        let id = serde_json::from_str(&row.get::<_, String>(0)?)?;
        result.push(OutlineRow {
            id,
            body: row.get(1)?,
            has_children: row.get(2)?,
        });
    }
    Ok(result)
}

fn parent_key(parent: Option<NodeId>) -> Result<String> {
    match parent {
        Some(id) => Ok(serde_json::to_string(&id)?),
        None => Ok(String::new()),
    }
}

pub(crate) enum TextTarget {
    Empty,
    Single { token: OpId, tip: OpId },
    Multiple,
}

pub(crate) fn edit_target(db: &Connection, node: NodeId) -> Result<Option<(String, TextTarget)>> {
    let mut query = db.prepare_cached(
        "SELECT note.body, outline.blank, note.token, note.tip
         FROM live_notes AS note JOIN live_outline AS outline ON outline.node=note.node
         WHERE note.node=?1",
    )?;
    let mut rows = query.query([serde_json::to_string(&node)?])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let target = match (
        row.get::<_, bool>(1)?,
        row.get::<_, Option<Vec<u8>>>(2)?,
        row.get::<_, Option<Vec<u8>>>(3)?,
    ) {
        (true, _, _) => TextTarget::Empty,
        (false, Some(token), Some(tip)) => TextTarget::Single {
            token: decode_op(&token)?,
            tip: decode_op(&tip)?,
        },
        _ => TextTarget::Multiple,
    };
    Ok(Some((row.get(0)?, target)))
}

pub(crate) fn has_stamp(db: &Connection, expected: &[u8]) -> Result<bool> {
    let stamp: Option<Vec<u8>> = db
        .prepare_cached("SELECT stamp FROM view_state WHERE singleton=1")?
        .query_row([], |row| row.get(0))
        .optional()?;
    Ok(stamp.as_deref() == Some(expected))
}

pub(crate) fn stamp_view(db: &Connection, stamp: &[u8]) -> Result<()> {
    db.prepare_cached("INSERT OR REPLACE INTO view_state VALUES (1, ?1)")?
        .execute([stamp])?;
    Ok(())
}

pub(crate) fn read_frontier(db: &Connection) -> Result<EditFrontier> {
    let value: String = db
        .prepare_cached("SELECT state FROM view_frontier WHERE singleton=1")?
        .query_row([], |row| row.get(0))?;
    Ok(serde_json::from_str(&value)?)
}

fn write_frontier(db: &Connection, frontier: &EditFrontier) -> Result<()> {
    db.prepare_cached("INSERT OR REPLACE INTO view_frontier VALUES (1, ?1)")?
        .execute([serde_json::to_string(frontier)?])?;
    Ok(())
}

pub(crate) fn replace_view(db: &mut Connection, stamp: &[u8], snapshot: &Snapshot) -> Result<()> {
    let transaction = db.transaction()?;
    transaction.execute_batch(
        "DELETE FROM live_notes; DELETE FROM live_outline; DELETE FROM known_nodes;",
    )?;
    let mut known = transaction.prepare("INSERT INTO known_nodes VALUES (?1)")?;
    for node in snapshot.known_nodes() {
        known.execute([serde_json::to_string(&node)?])?;
    }
    drop(known);
    let mut insert = transaction
        .prepare("INSERT INTO live_notes(node, body, token, tip) VALUES (?1, ?2, ?3, ?4)")?;
    let mut outline = transaction.prepare("INSERT INTO live_outline VALUES (?1, ?2, ?3, ?4)")?;
    let mut parents = vec![None];
    while let Some(parent) = parents.pop() {
        let parent_key = parent_key(parent)?;
        for (position, node) in snapshot.children(parent).into_iter().enumerate() {
            let body = snapshot
                .body(node)
                .ok_or("Visible Brain note has no body")?;
            let key = serde_json::to_string(&node)?;
            let winner = snapshot.single_text_winner(node);
            insert.execute(params![
                key,
                body,
                winner.map(|(token, _)| op_key(token)),
                winner.map(|(_, tip)| op_key(tip)),
            ])?;
            outline.execute(params![
                key,
                parent_key,
                i64::try_from(position)?,
                !snapshot.has_live_text(node),
            ])?;
            parents.push(Some(node));
        }
    }
    drop(insert);
    drop(outline);
    write_frontier(&transaction, snapshot.frontier())?;
    stamp_view(&transaction, stamp)?;
    transaction.commit()?;
    flush_wal(db)
}

pub(crate) fn apply_paged_edit(db: &Connection, edit: &Edit) -> Result<bool> {
    let mut frontier = read_frontier(db)?;
    if !frontier.complete || edit.id.clock <= frontier.clock {
        return Ok(false);
    }
    let applied = match &edit.change {
        Change::Create { node, place } => add_node(db, *node, *place)?,
        Change::CreateWithText { place, text } => {
            let node = NodeId::Note(edit.id);
            add_node(db, node, *place)? && insert_first_text(db, node, edit.id, text)?
        }
        Change::InsertText {
            node,
            after: None,
            text,
        } => insert_first_text(db, *node, edit.id, text)?,
        Change::EditText { .. } => apply_text(db, edit)?,
        _ => false,
    };
    if applied {
        frontier.observe(edit.id, &edit.dependencies);
        write_frontier(db, &frontier)?;
    }
    Ok(applied)
}

fn add_node(db: &Connection, node: NodeId, place: Place) -> Result<bool> {
    let key = serde_json::to_string(&node)?;
    let parent = parent_key(place.parent)?;
    let known: bool = db
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM known_nodes WHERE node=?1)")?
        .query_row([&key], |row| row.get(0))?;
    if known {
        return Ok(false);
    }
    if let Some(parent) = place.parent
        && read_note(db, parent)?.is_none()
    {
        return Ok(false);
    }
    let position = if let Some(anchor) = place.after {
        let anchor: Option<i64> = db
            .prepare_cached("SELECT position FROM live_outline WHERE node=?1 AND parent=?2")?
            .query_row(params![serde_json::to_string(&anchor)?, parent], |row| {
                row.get(0)
            })
            .optional()?;
        let Some(anchor) = anchor else {
            return Ok(false);
        };
        let position = anchor + 1;
        db.prepare_cached(
            "UPDATE live_outline SET position=position+1 WHERE parent=?1 AND position>=?2",
        )?
        .execute(params![parent, position])?;
        position
    } else {
        db.prepare_cached("SELECT coalesce(max(position)+1, 0) FROM live_outline WHERE parent=?1")?
            .query_row([&parent], |row| row.get(0))?
    };
    db.prepare_cached("INSERT INTO known_nodes VALUES (?1)")?
        .execute([&key])?;
    db.prepare_cached("INSERT INTO live_outline VALUES (?1, ?2, ?3, 1)")?
        .execute(params![key, parent, position])?;
    db.prepare_cached("INSERT INTO live_notes(node, body) VALUES (?1, '')")?
        .execute([key])?;
    Ok(true)
}

fn insert_first_text(db: &Connection, node: NodeId, id: OpId, text: &str) -> Result<bool> {
    let node = serde_json::to_string(&node)?;
    let count = db
        .prepare_cached("UPDATE live_outline SET blank=0 WHERE node=?1 AND blank=1")?
        .execute([&node])?;
    if count == 0 {
        return Ok(false);
    }
    db.prepare_cached("UPDATE live_notes SET body=?1, token=?2, tip=?2 WHERE node=?3")?
        .execute(params![text, op_key(id), node])?;
    Ok(true)
}

pub(crate) fn apply_text(db: &Connection, edit: &Edit) -> Result<bool> {
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

pub(crate) fn op_key(id: OpId) -> [u8; 16] {
    let mut key = [0; 16];
    key[..8].copy_from_slice(&id.clock.to_be_bytes());
    key[8..].copy_from_slice(&id.device.to_be_bytes());
    key
}

fn decode_op(key: &[u8]) -> Result<OpId> {
    let key: &[u8; 16] = key.try_into()?;
    Ok(OpId {
        clock: u64::from_be_bytes(key[..8].try_into()?),
        device: u64::from_be_bytes(key[8..].try_into()?),
    })
}

pub(crate) fn flush_wal(db: &Connection) -> Result<()> {
    let busy = db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        row.get::<_, i64>(0)
    })?;
    if busy != 0 {
        return Err("The Brain database was busy during checkpoint".into());
    }
    Ok(())
}
