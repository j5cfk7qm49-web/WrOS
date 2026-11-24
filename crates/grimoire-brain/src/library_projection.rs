//! Read models for works tagged in Brain and their versioned editions.

use rusqlite::{Connection, OptionalExtension, params};

use crate::library::{Edition, EditionKey, Filter, Sort, Work};
use crate::model::{Builtin, NodeId, Snapshot};
use crate::projection::{Result, op_key};

const SOURCE_KINDS: &str = "('Book','Article','Chapter','Lecture','Audio','Web')";

pub(crate) fn create_tables(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS library_editions(
             id TEXT PRIMARY KEY, work TEXT NOT NULL, creation BLOB NOT NULL,
             title TEXT NOT NULL, author TEXT NOT NULL, year INTEGER,
             added INTEGER NOT NULL, identifiers TEXT NOT NULL, data TEXT NOT NULL
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS library_editions_work ON library_editions(work,creation);",
    )?;
    Ok(())
}

pub(crate) fn rebuild(db: &Connection, snapshot: &Snapshot) -> Result<()> {
    db.execute("DELETE FROM library_editions", [])?;
    for (key, edition) in snapshot.library_editions() {
        save(db, key, edition)?;
    }
    Ok(())
}

pub(crate) fn save(db: &Connection, key: EditionKey, edition: &Edition) -> Result<()> {
    let identifiers = ["DOI", "ISBN", "ISSN"]
        .into_iter()
        .filter_map(|name| {
            edition
                .bibliography
                .get(name)
                .and_then(serde_json::Value::as_str)
        })
        .collect::<Vec<_>>()
        .join(" ");
    db.prepare_cached(
        "INSERT OR REPLACE INTO library_editions VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
    )?
    .execute(params![
        serde_json::to_string(&key)?,
        serde_json::to_string(&key.work)?,
        op_key(key.edition),
        edition.title(),
        edition.author(),
        edition.year(),
        edition.added_at,
        identifiers,
        serde_json::to_string(edition)?
    ])?;
    Ok(())
}

pub(crate) fn is_work(db: &Connection, work: NodeId) -> Result<bool> {
    Ok(db
        .prepare_cached(&format!(
            "SELECT EXISTS(SELECT 1 FROM live_notes JOIN note_tags AS t ON t.node=live_notes.node
         WHERE live_notes.node=?1 AND json_extract(t.target,'$.Builtin') IN {SOURCE_KINDS})",
        ))?
        .query_row([serde_json::to_string(&work)?], |row| row.get(0))?)
}

pub(crate) fn edition(db: &Connection, key: EditionKey) -> Result<Option<Edition>> {
    if !is_work(db, key.work)? {
        return Ok(None);
    }
    let raw: Option<String> = db
        .prepare_cached("SELECT data FROM library_editions WHERE id=?1")?
        .query_row([serde_json::to_string(&key)?], |row| row.get(0))
        .optional()?;
    Ok(raw.map(|json| serde_json::from_str(&json)).transpose()?)
}

pub(crate) fn for_work(db: &Connection, work: NodeId) -> Result<Vec<(EditionKey, Edition)>> {
    if !is_work(db, work)? {
        return Ok(Vec::new());
    }
    let mut query = db.prepare_cached(
        "SELECT id,data FROM library_editions WHERE work=?1 ORDER BY creation LIMIT 120",
    )?;
    fetch_editions(&mut query, [serde_json::to_string(&work)?])
}

pub(crate) fn find_editions(
    db: &Connection,
    text: &str,
    count: u16,
) -> Result<Vec<(EditionKey, Edition)>> {
    let mut query = db.prepare_cached(&format!(
        "SELECT e.id,e.data FROM library_editions AS e JOIN live_notes AS n ON n.node=e.work
         WHERE EXISTS(SELECT 1 FROM note_tags AS t WHERE t.node=e.work
             AND json_extract(t.target,'$.Builtin') IN {SOURCE_KINDS})
         AND instr(lower(e.title||' '||e.author||' '||e.identifiers||' '||n.body),lower(?1))>0
         ORDER BY e.title COLLATE NOCASE,e.creation LIMIT ?2",
    ))?;
    fetch_editions(&mut query, params![text.trim(), count.min(120)])
}

fn fetch_editions(
    query: &mut rusqlite::Statement<'_>,
    parameters: impl rusqlite::Params,
) -> Result<Vec<(EditionKey, Edition)>> {
    let mut rows = query.query(parameters)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push((
            serde_json::from_str(&row.get::<_, String>(0)?)?,
            serde_json::from_str(&row.get::<_, String>(1)?)?,
        ));
    }
    Ok(result)
}

pub(crate) fn works(
    db: &Connection,
    filter: &Filter,
    offset: u32,
    count: u16,
) -> Result<Vec<Work>> {
    let order = match filter.sort {
        Sort::Title => "title COLLATE NOCASE",
        Sort::Author => "author COLLATE NOCASE,title COLLATE NOCASE",
        Sort::Year => "year DESC,title COLLATE NOCASE",
        Sort::Type => "kind,title COLLATE NOCASE",
        Sort::Status => "status COLLATE NOCASE,title COLLATE NOCASE",
        Sort::Added => "added DESC,title COLLATE NOCASE",
    };
    let mut query = db.prepare_cached(&format!(
        "WITH works AS (
           SELECT t.node, min(json_extract(t.target,'$.Builtin')) AS kind
           FROM note_tags AS t WHERE json_extract(t.target,'$.Builtin') IN {SOURCE_KINDS}
             AND (?2 IS NULL OR t.target=?2) GROUP BY t.node
         )
         SELECT n.node, n.body AS title,
                COALESCE(NULLIF(e.author,''), metadata_author.value, '') AS author,
                COALESCE(e.year,CAST(metadata_year.value AS INTEGER)) AS year,
                w.kind, e.added AS added,
                (SELECT count(*) FROM library_editions WHERE work=n.node) AS editions,
                COALESCE(metadata_status.value,'') AS status
         FROM works AS w JOIN live_notes AS n ON n.node=w.node
         LEFT JOIN library_editions AS e ON e.id=(
             SELECT id FROM library_editions WHERE work=n.node ORDER BY creation LIMIT 1)
         LEFT JOIN note_fields AS metadata_author ON metadata_author.node=n.node
             AND metadata_author.field=json_object('Builtin',json_object('tag',w.kind,'slot',0))
         LEFT JOIN note_fields AS metadata_year ON metadata_year.node=n.node
             AND w.kind IN ('Book','Article','Chapter')
             AND metadata_year.field=json_object('Builtin',json_object('tag',w.kind,'slot',1))
         LEFT JOIN note_fields AS metadata_status ON metadata_status.node=n.node
             AND w.kind='Book'
             AND metadata_status.field=json_object('Builtin',json_object('tag','Book','slot',3))
         WHERE (?3 IS NULL OR EXISTS(SELECT 1 FROM note_tags WHERE node=w.node AND target=?3))
         AND (instr(lower(n.body || ' ' || coalesce(e.author,'') || ' ' || coalesce(metadata_author.value,'')
               || ' ' || coalesce(metadata_status.value,'')), lower(?1))>0
             OR EXISTS(SELECT 1 FROM library_editions AS other WHERE other.work=w.node
                  AND instr(lower(other.title||' '||other.author||' '||other.identifiers),lower(?1))>0))
         ORDER BY {order},n.node LIMIT ?4 OFFSET ?5",
    ))?;
    let type_key = filter
        .source_type
        .map(|tag| serde_json::to_string(&NodeId::Builtin(tag)))
        .transpose()?;
    let tag = filter
        .tag
        .map(|tag| serde_json::to_string(&tag))
        .transpose()?;
    let mut rows = query.query(params![
        filter.text.trim(),
        type_key,
        tag,
        count.min(120),
        offset
    ])?;
    let mut works = Vec::new();
    while let Some(row) = rows.next()? {
        let kind = row.get::<_, String>(4)?;
        let source_type: Builtin = serde_json::from_value(serde_json::Value::String(kind))?;
        works.push(Work {
            id: serde_json::from_str(&row.get::<_, String>(0)?)?,
            title: row.get(1)?,
            author: row.get(2)?,
            year: row.get(3)?,
            source_type,
            editions: row.get(6)?,
            status: row.get(7)?,
        });
    }
    Ok(works)
}
