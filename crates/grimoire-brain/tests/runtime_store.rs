#![cfg(feature = "runtime")]

use chrono::NaiveDate;
use grimoire_brain::bounded_store::PagedBrain;
use grimoire_brain::model::{NodeId, Place};

#[test]
fn today_and_local_notes_keep_their_identity_and_order_after_reopen() {
    let fixture = tempfile::tempdir().unwrap();
    let home = fixture.path().join("Brain");
    let cache = fixture.path().join("cache");
    let date = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
    let mut brain = PagedBrain::open_library(&home, &cache).unwrap();
    let today = brain.ensure_today(date).unwrap();
    assert_eq!(today, NodeId::Today(date));
    let child = |after| Place {
        parent: Some(today),
        after,
    };
    let first = brain.create_note(child(None)).unwrap();
    let last = brain.create_note(child(None)).unwrap();
    let middle = brain.create_note(child(Some(first))).unwrap();
    assert!(brain.set_note(middle, "Saved text").unwrap());
    assert!(!brain.set_note(middle, "Saved text").unwrap());
    assert_eq!(brain.children(Some(today), 0, 1).unwrap()[0].id, first);
    let rows = brain.children(Some(today), 1, 3).unwrap();
    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        [middle, last]
    );
    assert_eq!(rows[0].body, "Saved text");
    brain.checkpoint().unwrap();
    drop(brain);

    assert!(home.join("store/CURRENT").is_file());
    assert!(!home.join("store/live.sqlite").exists());
    assert!(cache.join("live.sqlite").is_file());
    let mut brain = PagedBrain::open_library(&home, &cache).unwrap();
    assert_eq!(brain.ensure_today(date).unwrap(), today);
    let fourth = brain.create_note(child(None)).unwrap();
    let (NodeId::Note(first_id), NodeId::Note(fourth_id)) = (first, fourth) else {
        panic!("Local notes use operation identities");
    };
    assert_eq!(first_id.device, fourth_id.device);
    assert!(fourth_id.clock > first_id.clock);
    assert_eq!(brain.note(middle).unwrap().as_deref(), Some("Saved text"));
}

#[test]
fn deleting_the_projection_rebuilds_it_from_the_checked_store() {
    let fixture = tempfile::tempdir().unwrap();
    let home = fixture.path().join("Brain");
    let cache = fixture.path().join("cache");
    let mut brain = PagedBrain::open_library(&home, &cache).unwrap();
    let node = brain.create_note(Place::default()).unwrap();
    brain.set_note(node, "Rebuild this text").unwrap();
    brain.checkpoint().unwrap();
    drop(brain);
    std::fs::remove_file(cache.join("live.sqlite")).unwrap();

    let mut brain = PagedBrain::open_library(&home, &cache).unwrap();
    assert_eq!(
        brain.note(node).unwrap().as_deref(),
        Some("Rebuild this text")
    );
    let rows = brain.children(None, 0, 10).unwrap();
    assert_eq!(rows[0].id, node);
    assert_eq!(rows[0].body, "Rebuild this text");
    assert_eq!(brain.project().unwrap().children(None), [node]);
}
