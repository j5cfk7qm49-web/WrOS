#![cfg(test)]
use chrono::{DateTime, FixedOffset};
use grimoire_brain::model::{NodeId, OpId};
use grimoire_brain::review::{CardKey, Review, Schedule, cards_for_text, update_schedule};

fn op(clock: u64, device: u64) -> OpId {
    OpId { clock, device }
}

fn time(value: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(value).unwrap()
}

#[test]
fn edited_and_reordered_cards_keep_their_ids() {
    let initial = cards_for_text("{{malate}} is made from {{fumarate}}.", &[], op(1, 2));
    let reordered = cards_for_text(
        "From {{fumarate}}, the intermediate is {{malate}}.",
        &initial,
        op(2, 2),
    );
    assert_eq!(initial[0].id, reordered[1].id);
    assert_eq!(initial[1].id, reordered[0].id);
    let edited = cards_for_text(
        "From {{fumarate}}, the corrected spelling is {{L-malate}}.",
        &reordered,
        op(3, 2),
    );
    assert_eq!(initial[0].id, edited[1].id);
    assert_eq!(edited[1].answer, "L-malate");
    assert!(edited[1].prompt.contains("[…]"));
}

#[test]
fn removing_one_of_two_identical_cloze_answers_preserves_the_other_grade_history() {
    let initial = cards_for_text(
        "From {{north}} pass {{north}} then {{south}}.",
        &[],
        op(1, 2),
    );
    let edited = cards_for_text(
        "From north pass {{north}} then {{south}}.",
        &initial,
        op(2, 2),
    );
    assert_eq!(edited.len(), 2);
    assert_eq!(edited[0], initial[1]);
    assert_eq!(edited[1], initial[2]);
}

#[test]
fn two_way_and_outline_answers_keep_their_roles() {
    let cards = cards_for_text("mistletoe ::: gui", &[], op(7, 2));
    assert_eq!(cards.len(), 2);
    let edited = cards_for_text("Mistletoe ::: le gui", &cards, op(8, 2));
    assert_eq!(cards[0].id, edited[0].id);
    assert_eq!(cards[1].id, edited[1].id);
    assert_eq!(edited[1].prompt, "le gui");
    assert_eq!(edited[1].answer, "Mistletoe");
    let outline = cards_for_text("Order of operations ::", &[], op(9, 2));
    assert_eq!(outline[0].prompt, "Order of operations");
    assert!(outline[0].answer.is_empty());
}

#[test]
fn original_grade_and_local_day_survive_a_clock_reversal() {
    let card = CardKey {
        node: NodeId::Note(op(2, 7)),
        id: cards_for_text("首都 :: 京城", &[], op(2, 7))[0].id,
    };
    let first = Review::capture(card, 5, time("2026-11-22T23:58:00-08:00"), None).unwrap();
    let next = Review::capture(
        card,
        0,
        time("2026-11-22T23:40:00-08:00"),
        Some(first.effective_at),
    )
    .unwrap();
    assert_eq!(next.grade, 0);
    assert_eq!(next.effective_at, first.effective_at);
    assert_eq!(next.observed_at.offset().local_minus_utc(), -28800);
    assert_eq!(next.observed_at.date_naive().to_string(), "2026-11-22");
    let forward = update_schedule(Some(update_schedule(None, &first).unwrap()), &next).unwrap();
    assert!(forward.due_at > next.effective_at);
    assert_eq!(forward.reviews, 2);
    assert!(Review::capture(card, 6, first.observed_at, None).is_err());
}

#[test]
fn merged_replay_deduplicates_and_ignores_arrival_order() {
    let node = NodeId::Note(op(1, 4));
    let card = CardKey {
        node,
        id: cards_for_text("question :: answer", &[], op(1, 4))[0].id,
    };
    let first = Review::capture(card, 4, time("2026-11-22T19:45:00+09:00"), None).unwrap();
    let next = Review::capture(card, 2, time("2026-11-22T02:45:00-08:00"), None).unwrap();
    let last = Review::capture(card, 3, time("2026-11-22T23:05:00-08:00"), None).unwrap();
    let a = [(op(7, 2), &next), (op(8, 2), &last), (op(7, 1), &first)];
    let b = [(a[2]), (a[0]), (a[1]), (a[2])];
    let left = Schedule::replay(a).unwrap();
    assert_eq!(left, Schedule::replay(b).unwrap());
    assert_eq!(left.unwrap().reviews, 3);
}

#[cfg(feature = "runtime")]
#[test]
fn card_state_rebuilds_from_note_edits_and_appended_grades() {
    use grimoire_brain::bounded_store::PagedBrain;
    use grimoire_brain::model::{Builtin, Place};
    use grimoire_brain::schema::SavedQuery;

    let tmp = tempfile::tempdir().unwrap();
    let mut store = PagedBrain::open(tmp.path(), 1).unwrap();
    let tag = NodeId::Builtin(Builtin::Concept);
    let node = store
        .create_tagged_note(Place::default(), "{{one}} goes before {{two}}", Some(tag))
        .unwrap();
    let tag_filter = SavedQuery::Tagged(tag);
    let at = time("2026-11-22T23:59:00-08:00");
    let due = store.due_cards(at.timestamp(), Some(&tag_filter)).unwrap();
    assert_eq!(due.count, 2);
    let first = due.next.unwrap();
    assert_eq!(first.answer, "one");
    store.grade_card(first.key, 4, at).unwrap();
    store
        .set_note(node, "After {{two}}, recall {{one}}")
        .unwrap();
    assert_eq!(
        store
            .due_cards(at.timestamp(), None)
            .unwrap()
            .next
            .unwrap()
            .answer,
        "two",
    );
    drop(store);

    std::fs::remove_file(tmp.path().join("live.sqlite")).unwrap();
    let mut reopened = PagedBrain::open(tmp.path(), 1).unwrap();
    let filtered = reopened
        .due_cards(at.timestamp(), Some(&tag_filter))
        .unwrap();
    assert_eq!(filtered.count, 1);
    let second = filtered.next.unwrap();
    assert_eq!(second.answer, "two");
    assert_ne!(second.key, first.key);
    let text_search = SavedQuery::Text("recall".into());
    assert_eq!(
        reopened
            .due_cards(at.timestamp(), Some(&text_search))
            .unwrap()
            .count,
        1,
    );
    let snapshot = reopened.project().unwrap();
    let (event_id, event) = snapshot.review_events().next().unwrap();
    assert_eq!(event_id.device, 1);
    assert_eq!(event.card, first.key);
    assert_eq!(event.grade, 4);
    assert_eq!(event.observed_at.offset().local_minus_utc(), -28800);
}

#[cfg(feature = "runtime")]
#[test]
fn outline_children_supply_the_answer() {
    use grimoire_brain::bounded_store::PagedBrain;
    use grimoire_brain::model::Place;

    let tmp = tempfile::tempdir().unwrap();
    let mut store = PagedBrain::open(tmp.path(), 2).unwrap();
    let node = store
        .create_note_with_text(Place::default(), "Steps ::")
        .unwrap();
    assert_eq!(store.due_cards(0, None).unwrap().count, 0);
    let placement = Place {
        parent: Some(node),
        after: None,
    };
    store
        .create_note_with_text(placement, "Find the item")
        .unwrap();
    store
        .create_note_with_text(placement, "Use the item")
        .unwrap();
    let next = store.due_cards(0, None).unwrap().next.unwrap();
    assert_eq!(next.answer, "• Find the item\n• Use the item");
}

#[cfg(feature = "runtime")]
#[test]
fn an_rc25_cache_gets_cards_without_replaying_the_outline() {
    use grimoire_brain::bounded_store::PagedBrain;
    use grimoire_brain::model::Place;
    use rusqlite::Connection;

    let tmp = tempfile::tempdir().unwrap();
    let id = {
        let mut store = PagedBrain::open(tmp.path(), 8).unwrap();
        let node = store
            .create_note_with_text(Place::default(), "Older note :: Answer")
            .unwrap();
        assert_eq!(store.due_cards(0, None).unwrap().count, 1);
        node
    };
    let cache = Connection::open(tmp.path().join("live.sqlite")).unwrap();
    let mut stamp: Vec<u8> = cache
        .query_row("SELECT stamp FROM view_state", [], |row| row.get(0))
        .unwrap();
    assert!(stamp.starts_with(b"paged-outline-v6\0"));
    stamp[b"paged-outline-v".len()] = b'4';
    cache
        .execute("UPDATE view_state SET stamp=?1", [&stamp])
        .unwrap();
    cache.execute("DELETE FROM review_cards", []).unwrap();
    drop(cache);
    let mut store = PagedBrain::open(tmp.path(), 8).unwrap();
    let card = store.due_cards(0, None).unwrap().next.unwrap();
    assert_eq!(card.key.node, id);
    assert_eq!(card.answer, "Answer");
}
