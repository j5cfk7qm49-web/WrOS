//! Derived, bounded lists over live notes. The journal remains authoritative.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, Params, params};
use serde::de::DeserializeOwned;

use crate::model::{Change, FieldId, NodeId, Snapshot};
use crate::projection::{OutlineRow, Result};
use crate::schema::{FieldSpec, SavedQuery, default_fields};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteDetails {
    pub tags: Vec<NodeId>,
    pub links: Vec<NodeId>,
    pub fields: Vec<(FieldId, String)>,
    pub query: Option<SavedQuery>,
}

pub(crate) fn create_tables(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS note_tags(
            node TEXT NOT NULL, target TEXT NOT NULL, PRIMARY KEY(node, target)
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS tagged_notes ON note_tags(target, node);
         CREATE TABLE IF NOT EXISTS note_links(
            node TEXT NOT NULL, target TEXT NOT NULL, PRIMARY KEY(node, target)
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS linked_notes ON note_links(target, node);
         CREATE TABLE IF NOT EXISTS note_fields(
            node TEXT NOT NULL, field TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY(node, field)
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS field_values ON note_fields(field, value, node);
         CREATE TABLE IF NOT EXISTS tag_schema(
            tag TEXT NOT NULL, field TEXT NOT NULL, spec TEXT NOT NULL, PRIMARY KEY(tag, field)
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS note_queries(node TEXT PRIMARY KEY, query TEXT NOT NULL)
         WITHOUT ROWID;
         CREATE VIRTUAL TABLE IF NOT EXISTS brain_fts USING fts5(
            body, content='', contentless_delete=1
         );
         CREATE TRIGGER IF NOT EXISTS brain_fts_insert AFTER INSERT ON live_notes BEGIN
            INSERT INTO brain_fts(rowid, body) VALUES (NEW.rowid, NEW.body);
         END;
         CREATE TRIGGER IF NOT EXISTS brain_fts_update AFTER UPDATE OF body ON live_notes
         WHEN OLD.body <> NEW.body BEGIN
            DELETE FROM brain_fts WHERE rowid=OLD.rowid;
            INSERT INTO brain_fts(rowid, body) VALUES (NEW.rowid, NEW.body);
         END;
         CREATE TRIGGER IF NOT EXISTS brain_fts_delete AFTER DELETE ON live_notes BEGIN
            DELETE FROM brain_fts WHERE rowid=OLD.rowid;
         END;",
    )?;
    Ok(())
}

pub(crate) fn reset(db: &Connection) -> Result<()> {
    db.execute_batch(
        "INSERT INTO brain_fts(brain_fts) VALUES ('delete-all');
         DELETE FROM note_tags; DELETE FROM note_links; DELETE FROM note_fields;
         DELETE FROM tag_schema; DELETE FROM note_queries;",
    )?;
    Ok(())
}

pub(crate) fn populate_note(db: &Connection, snapshot: &Snapshot, node: NodeId) -> Result<()> {
    let key = serde_json::to_string(&node)?;
    for tag in snapshot.tags(node) {
        set_relation(db, Relation::Tag, &key, tag, true)?;
    }
    for target in snapshot.links(node) {
        set_relation(db, Relation::Link, &key, target, true)?;
    }
    for (field, value) in snapshot.fields(node) {
        set_field(db, &key, field, Some(value))?;
    }
    if let Some(query) = snapshot.saved_search(node) {
        set_query(db, &key, Some(query))?;
    }
    Ok(())
}

pub(crate) fn populate_schema(db: &Connection, snapshot: &Snapshot, tag: NodeId) -> Result<()> {
    let defaults = default_fields(tag);
    for (field, spec) in snapshot.fields_for_tag(tag) {
        if defaults.get(&field) != Some(&spec) {
            set_schema(db, tag, field, &spec)?;
        }
    }
    Ok(())
}

pub(crate) fn apply(db: &Connection, change: &Change) -> Result<bool> {
    match change {
        Change::SetTag { node, tag, present } => set_relation(
            db,
            Relation::Tag,
            &serde_json::to_string(node)?,
            *tag,
            *present,
        )?,
        Change::SetLink {
            node,
            target,
            present,
        } => set_relation(
            db,
            Relation::Link,
            &serde_json::to_string(node)?,
            *target,
            *present,
        )?,
        Change::SetField { node, field, value } => {
            set_field(db, &serde_json::to_string(node)?, *field, value.as_deref())?
        }
        Change::SetSearch { node, query } => {
            set_query(db, &serde_json::to_string(node)?, query.as_ref())?
        }
        Change::DefineField { tag, field, spec } => set_schema(db, *tag, *field, spec)?,
        _ => return Ok(false),
    }
    Ok(true)
}

enum Relation {
    Tag,
    Link,
}

fn set_relation(
    db: &Connection,
    relation: Relation,
    node: &str,
    target: NodeId,
    present: bool,
) -> Result<()> {
    let sql = match (relation, present) {
        (Relation::Tag, true) => "INSERT OR IGNORE INTO note_tags VALUES (?1, ?2)",
        (Relation::Link, true) => "INSERT OR IGNORE INTO note_links VALUES (?1, ?2)",
        (Relation::Tag, false) => "DELETE FROM note_tags WHERE node=?1 AND target=?2",
        (Relation::Link, false) => "DELETE FROM note_links WHERE node=?1 AND target=?2",
    };
    db.prepare_cached(sql)?
        .execute(params![node, serde_json::to_string(&target)?])?;
    Ok(())
}

fn set_field(db: &Connection, node: &str, field: FieldId, value: Option<&str>) -> Result<()> {
    let field = serde_json::to_string(&field)?;
    if let Some(value) = value {
        db.prepare_cached("INSERT OR REPLACE INTO note_fields VALUES (?1, ?2, ?3)")?
            .execute(params![node, field, value])?;
    } else {
        db.prepare_cached("DELETE FROM note_fields WHERE node=?1 AND field=?2")?
            .execute(params![node, field])?;
    }
    Ok(())
}

fn set_schema(db: &Connection, tag: NodeId, field: FieldId, spec: &FieldSpec) -> Result<()> {
    db.prepare_cached("INSERT OR REPLACE INTO tag_schema VALUES (?1, ?2, ?3)")?
        .execute(params![
            serde_json::to_string(&tag)?,
            serde_json::to_string(&field)?,
            serde_json::to_string(spec)?
        ])?;
    Ok(())
}

fn set_query(db: &Connection, node: &str, query: Option<&SavedQuery>) -> Result<()> {
    if let Some(query) = query {
        db.prepare_cached("INSERT OR REPLACE INTO note_queries VALUES (?1, ?2)")?
            .execute(params![node, serde_json::to_string(query)?])?;
    } else {
        db.prepare_cached("DELETE FROM note_queries WHERE node=?1")?
            .execute([node])?;
    }
    Ok(())
}

fn identifiers<T: DeserializeOwned>(db: &Connection, sql: &str, node: &str) -> Result<Vec<T>> {
    let mut query = db.prepare_cached(sql)?;
    let mut rows = query.query([node])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
    }
    Ok(result)
}

pub(crate) fn details(db: &Connection, node: NodeId) -> Result<NoteDetails> {
    let node = serde_json::to_string(&node)?;
    let tags = identifiers(
        db,
        "SELECT target FROM note_tags WHERE node=?1 ORDER BY target",
        &node,
    )?;
    let links = identifiers(
        db,
        "SELECT target FROM note_links WHERE node=?1 ORDER BY target",
        &node,
    )?;
    let mut statement =
        db.prepare_cached("SELECT field, value FROM note_fields WHERE node=?1 ORDER BY field")?;
    let mut fields = Vec::new();
    let mut rows = statement.query([&node])?;
    while let Some(row) = rows.next()? {
        fields.push((
            serde_json::from_str(&row.get::<_, String>(0)?)?,
            row.get(1)?,
        ));
    }
    let query = db
        .prepare_cached("SELECT query FROM note_queries WHERE node=?1")?
        .query_row([&node], |row| row.get::<_, String>(0))
        .optional()?
        .map(|raw| serde_json::from_str(&raw))
        .transpose()?;
    Ok(NoteDetails {
        tags,
        links,
        fields,
        query,
    })
}

pub(crate) fn fields_for_tag(db: &Connection, tag: NodeId) -> Result<BTreeMap<FieldId, FieldSpec>> {
    let mut fields = default_fields(tag);
    let mut query =
        db.prepare_cached("SELECT field, spec FROM tag_schema WHERE tag=?1 ORDER BY field")?;
    let mut rows = query.query([serde_json::to_string(&tag)?])?;
    while let Some(row) = rows.next()? {
        fields.insert(
            serde_json::from_str(&row.get::<_, String>(0)?)?,
            serde_json::from_str(&row.get::<_, String>(1)?)?,
        );
    }
    Ok(fields)
}

pub(crate) fn search(
    db: &Connection,
    query: &SavedQuery,
    offset: u32,
    count: u16,
) -> Result<Vec<OutlineRow>> {
    const TAGS: &str =
        "SELECT notes.node, notes.body, EXISTS(SELECT 1 FROM live_outline WHERE parent=notes.node)
        FROM note_tags AS hit JOIN live_notes AS notes ON notes.node=hit.node
        WHERE hit.target=?1 ORDER BY hit.node LIMIT ?2 OFFSET ?3";
    const LINKS: &str =
        "SELECT notes.node, notes.body, EXISTS(SELECT 1 FROM live_outline WHERE parent=notes.node)
        FROM note_links AS hit JOIN live_notes AS notes ON notes.node=hit.node
        WHERE hit.target=?1 ORDER BY hit.node LIMIT ?2 OFFSET ?3";
    const OUTGOING: &str =
        "SELECT notes.node, notes.body, EXISTS(SELECT 1 FROM live_outline WHERE parent=notes.node)
        FROM note_links AS hit JOIN live_notes AS notes ON notes.node=hit.target
        WHERE hit.node=?1 ORDER BY hit.target LIMIT ?2 OFFSET ?3";
    const FIELDS: &str =
        "SELECT notes.node, notes.body, EXISTS(SELECT 1 FROM live_outline WHERE parent=notes.node)
        FROM note_fields AS hit JOIN live_notes AS notes ON notes.node=hit.node
        WHERE hit.field=?1 AND hit.value=?4 ORDER BY hit.node LIMIT ?2 OFFSET ?3";
    const TEXT: &str = "WITH candidates AS MATERIALIZED (
        SELECT rowid, rank FROM brain_fts WHERE brain_fts MATCH ?1 ORDER BY rowid LIMIT 128
        ) SELECT notes.node, notes.body, EXISTS(SELECT 1 FROM live_outline WHERE parent=notes.node)
        FROM candidates JOIN live_notes AS notes ON notes.rowid=candidates.rowid
        ORDER BY candidates.rank, candidates.rowid LIMIT ?2 OFFSET ?3";
    let count = count.min(120);
    match query {
        SavedQuery::Text(value) => {
            let Some(pattern) = literal_phrase(value)? else {
                return Ok(Vec::new());
            };
            results(db, TEXT, params![pattern, count, offset])
        }
        SavedQuery::Tagged(tag) => results(
            db,
            TAGS,
            params![serde_json::to_string(tag)?, count, offset],
        ),
        SavedQuery::LinksTo(target) => results(
            db,
            LINKS,
            params![serde_json::to_string(target)?, count, offset],
        ),
        SavedQuery::LinksFrom(node) => results(
            db,
            OUTGOING,
            params![serde_json::to_string(node)?, count, offset],
        ),
        SavedQuery::Field { field, value } => results(
            db,
            FIELDS,
            params![serde_json::to_string(field)?, count, offset, value],
        ),
    }
}

pub(crate) fn backlink_count(db: &Connection, target: NodeId) -> Result<u64> {
    let mut query = db.prepare_cached(
        "SELECT count(*) FROM note_links AS hit JOIN live_notes AS notes ON notes.node=hit.node
         WHERE hit.target=?1",
    )?;
    let count: i64 = query.query_row([serde_json::to_string(&target)?], |row| row.get(0))?;
    Ok(u64::try_from(count)?)
}

pub(crate) fn literal_phrase(value: &str) -> Result<Option<String>> {
    let value = value.trim();
    if value.len() > 1024 {
        return Err("Search text is too long".into());
    }
    // Punctuation and quotes cannot change the expression or invoke operators.
    Ok((!value.is_empty()).then(|| format!("\"{}\"", value.replace('"', "\"\""))))
}

fn results(db: &Connection, sql: &str, parameters: impl Params) -> Result<Vec<OutlineRow>> {
    let mut query = db.prepare_cached(sql)?;
    let mut rows = query.query(parameters)?;
    let mut matches = Vec::new();
    while let Some(row) = rows.next()? {
        matches.push(OutlineRow {
            id: serde_json::from_str(&row.get::<_, String>(0)?)?,
            body: row.get(1)?,
            has_children: row.get(2)?,
        });
    }
    Ok(matches)
}
