//! Rebuildable card definitions, due schedules and the review event lookup.

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::model::{NodeId, OpId, Snapshot};
use crate::projection::{Result, op_key, stamp_view};
use crate::review::{Card, CardKey, Review, Schedule, cards_for_text, update_schedule};
use crate::schema::SavedQuery;
use crate::search;

const HAS_CHILD_ANSWER: &str = "EXISTS(
    SELECT 1 FROM live_outline AS item JOIN live_notes AS note ON note.node=item.node
    WHERE item.parent=c.node AND item.blank=0 AND trim(note.body)<>''
)";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DueCards {
    pub count: u64,
    pub next: Option<DueCard>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueCard {
    pub key: CardKey,
    pub prompt: String,
    pub answer: String,
}

pub(crate) fn create_tables(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS review_cards(
            card TEXT PRIMARY KEY, node TEXT NOT NULL,
            definition TEXT NOT NULL, from_children INTEGER NOT NULL,
            ordinal INTEGER NOT NULL
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS review_cards_note ON review_cards(node);
         CREATE TABLE IF NOT EXISTS review_events(
            id BLOB PRIMARY KEY, card TEXT NOT NULL, device BLOB NOT NULL,
            effective INTEGER NOT NULL, event TEXT NOT NULL
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS review_events_card
             ON review_events(card,effective,id);
         CREATE INDEX IF NOT EXISTS review_events_device
             ON review_events(card,device,effective);
         CREATE TABLE IF NOT EXISTS review_schedules(
            card TEXT PRIMARY KEY, due INTEGER NOT NULL, state TEXT NOT NULL
         ) WITHOUT ROWID;",
    )?;
    Ok(())
}

pub(crate) fn reset(db: &Connection) -> Result<()> {
    db.execute_batch(
        "DELETE FROM review_cards; DELETE FROM review_events; DELETE FROM review_schedules;",
    )?;
    Ok(())
}

pub(crate) fn upgrade_rc25(db: &mut Connection, stamp: &[u8]) -> Result<()> {
    let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    reset(&transaction)?;
    {
        let mut notes = transaction.prepare(
            "SELECT node,body FROM live_notes WHERE instr(body,' ::')>0 OR instr(body,'{{')>0",
        )?;
        let mut rows = notes.query([])?;
        while let Some(row) = rows.next()? {
            let node = serde_json::from_str(&row.get::<_, String>(0)?)?;
            let body: String = row.get(1)?;
            let cards = cards_for_text(&body, &[], initial_identity(node));
            if !cards.is_empty() {
                save_cards(&transaction, node, &cards)?;
            }
        }
    }
    stamp_view(&transaction, stamp)?;
    transaction.commit()?;
    Ok(())
}

pub(crate) fn save_cards(db: &Connection, node: NodeId, cards: &[Card]) -> Result<()> {
    let node_key = serde_json::to_string(&node)?;
    db.prepare_cached("DELETE FROM review_cards WHERE node=?1")?
        .execute([&node_key])?;
    let mut insert = db.prepare_cached("INSERT INTO review_cards VALUES (?1,?2,?3,?4,?5)")?;
    for (ordinal, card) in cards.iter().enumerate() {
        let key = CardKey { node, id: card.id };
        insert.execute(params![
            serde_json::to_string(&key)?,
            node_key,
            serde_json::to_string(card)?,
            card.answer.is_empty(),
            i64::try_from(ordinal)?
        ])?;
    }
    Ok(())
}

pub(crate) fn populate_note(
    db: &Connection,
    snapshot: &Snapshot,
    node: NodeId,
    body: &str,
) -> Result<()> {
    let fallback;
    let cards = match snapshot.saved_cards(node) {
        Some(saved) => saved,
        None => {
            fallback = cards_for_text(body, &[], initial_identity(node));
            &fallback
        }
    };
    if !cards.is_empty() {
        save_cards(db, node, cards)?;
    }
    Ok(())
}

pub(crate) fn initial_identity(node: NodeId) -> OpId {
    match node {
        NodeId::Note(id) => id,
        _ => OpId {
            clock: 0,
            device: 0,
        },
    }
}

pub(crate) fn note_cards(db: &Connection, node: NodeId) -> Result<Vec<Card>> {
    let mut query =
        db.prepare_cached("SELECT definition FROM review_cards WHERE node=?1 ORDER BY ordinal")?;
    let mut rows = query.query([serde_json::to_string(&node)?])?;
    let mut cards = Vec::new();
    while let Some(row) = rows.next()? {
        cards.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
    }
    Ok(cards)
}

pub(crate) fn last_on_device(db: &Connection, card: CardKey, device: u64) -> Result<Option<i64>> {
    Ok(db
        .prepare_cached("SELECT max(effective) FROM review_events WHERE card=?1 AND device=?2")?
        .query_row(
            params![serde_json::to_string(&card)?, device.to_be_bytes()],
            |row| row.get(0),
        )?)
}

pub(crate) fn is_reviewable(db: &Connection, card: CardKey) -> Result<bool> {
    Ok(db
        .prepare_cached(&format!(
            "SELECT EXISTS(SELECT 1 FROM review_cards AS c WHERE c.card=?1
                AND (NOT c.from_children OR {HAS_CHILD_ANSWER}))",
        ))?
        .query_row([serde_json::to_string(&card)?], |row| row.get(0))?)
}

fn insert_event(db: &Connection, id: OpId, card: &str, event: &Review) -> Result<()> {
    db.prepare_cached("INSERT INTO review_events VALUES (?1,?2,?3,?4,?5)")?
        .execute(params![
            op_key(id),
            card,
            id.device.to_be_bytes(),
            event.effective_at,
            serde_json::to_string(event)?,
        ])?;
    Ok(())
}

fn save_schedule(db: &Connection, card: &str, schedule: Schedule) -> Result<()> {
    db.prepare_cached(
        "INSERT INTO review_schedules VALUES (?1,?2,?3)
         ON CONFLICT(card) DO UPDATE SET due=excluded.due,state=excluded.state",
    )?
    .execute(params![
        card,
        schedule.due_at,
        serde_json::to_string(&schedule)?
    ])?;
    Ok(())
}

fn replay_card(db: &Connection, card: &str) -> Result<()> {
    let mut query = db
        .prepare_cached("SELECT id,event FROM review_events WHERE card=?1 ORDER BY effective,id")?;
    let mut rows = query.query([card])?;
    let mut history = Vec::new();
    while let Some(row) = rows.next()? {
        history.push((
            crate::projection::decode_op(&row.get::<_, Vec<u8>>(0)?)?,
            serde_json::from_str(&row.get::<_, String>(1)?)?,
        ));
    }
    if let Some(schedule) = Schedule::replay(history.iter().map(|(id, event)| (*id, event)))? {
        save_schedule(db, card, schedule)?;
    }
    Ok(())
}

pub(crate) fn rebuild_events(db: &Connection, snapshot: &Snapshot) -> Result<()> {
    let mut affected = BTreeSet::new();
    for (id, event) in snapshot.review_events() {
        let card = serde_json::to_string(&event.card)?;
        insert_event(db, id, &card, event)?;
        affected.insert(card);
    }
    for card in affected {
        replay_card(db, &card)?;
    }
    Ok(())
}

pub(crate) fn append_event(db: &Connection, id: OpId, event: &Review) -> Result<()> {
    let card = serde_json::to_string(&event.card)?;
    let previous: Option<String> = db
        .prepare_cached("SELECT state FROM review_schedules WHERE card=?1")?
        .query_row([&card], |row| row.get(0))
        .optional()?;
    let previous: Option<Schedule> = previous
        .map(|state| serde_json::from_str(&state))
        .transpose()?;
    insert_event(db, id, &card, event)?;
    if previous.is_some_and(|schedule| schedule.last.effective_at > event.effective_at) {
        replay_card(db, &card)
    } else {
        save_schedule(db, &card, update_schedule(previous, event)?)
    }
}

pub(crate) fn due(db: &Connection, now: i64, filter: Option<&SavedQuery>) -> Result<DueCards> {
    // These fixed predicates all compare against the same indexes as Brain's
    // lists. Only parameter values come from notes or keyboard input.
    let (predicate, key, value) = match filter {
        None => ("1", String::new(), String::new()),
        Some(SavedQuery::Tagged(tag)) => (
            "EXISTS(SELECT 1 FROM note_tags WHERE node=c.node AND target=?2)",
            serde_json::to_string(tag)?,
            String::new(),
        ),
        Some(SavedQuery::LinksTo(target)) => (
            "EXISTS(SELECT 1 FROM note_links WHERE node=c.node AND target=?2)",
            serde_json::to_string(target)?,
            String::new(),
        ),
        Some(SavedQuery::LinksFrom(node)) => (
            "EXISTS(SELECT 1 FROM note_links WHERE target=c.node AND node=?2)",
            serde_json::to_string(node)?,
            String::new(),
        ),
        Some(SavedQuery::Field { field, value }) => (
            "EXISTS(SELECT 1 FROM note_fields WHERE node=c.node AND field=?2 AND value=?3)",
            serde_json::to_string(field)?,
            value.clone(),
        ),
        Some(SavedQuery::Text(text)) => {
            let Some(pattern) = search::literal_phrase(text)? else {
                return Ok(DueCards::default());
            };
            (
                "EXISTS(SELECT 1 FROM live_notes JOIN brain_fts ON brain_fts.rowid=live_notes.rowid
                    WHERE live_notes.node=c.node AND brain_fts MATCH ?2)",
                pattern,
                String::new(),
            )
        }
    };
    let mut selection = db.prepare_cached(&format!(
        "SELECT c.card,c.definition,count(*) OVER() FROM review_cards AS c
         LEFT JOIN review_schedules AS s ON s.card=c.card
         WHERE (s.due IS NULL OR s.due<=?1) AND {predicate}
           AND (NOT c.from_children OR {HAS_CHILD_ANSWER})
           AND ?2 IS NOT NULL AND ?3 IS NOT NULL
         ORDER BY coalesce(s.due,-9223372036854775808),c.card LIMIT 1",
    ))?;
    let raw = selection
        .query_row(params![now, key, value], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .optional()?;
    let Some((card, definition, count)) = raw else {
        return Ok(DueCards::default());
    };
    let key: CardKey = serde_json::from_str(&card)?;
    let mut card: Card = serde_json::from_str(&definition)?;
    if card.answer.is_empty() {
        let mut children = db.prepare_cached(
            "SELECT note.body FROM live_outline AS item JOIN live_notes AS note ON note.node=item.node
             WHERE item.parent=?1 AND item.blank=0 AND trim(note.body)<>'' ORDER BY item.position LIMIT 120",
        )?;
        let mut rows = children.query([serde_json::to_string(&key.node)?])?;
        while let Some(row) = rows.next()? {
            if !card.answer.is_empty() {
                card.answer.push('\n');
            }
            card.answer.push_str("• ");
            card.answer.push_str(&row.get::<_, String>(0)?);
        }
    }
    Ok(DueCards {
        count: u64::try_from(count)?,
        next: Some(DueCard {
            key,
            prompt: card.prompt,
            answer: card.answer,
        }),
    })
}
