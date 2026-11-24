#![cfg(feature = "runtime")]

use chrono::NaiveDate;
use grimoire_brain::bounded_store::PagedBrain;
use grimoire_brain::model::{Builtin, FieldId, NodeId, Place};
use grimoire_brain::schema::{FieldKind, FieldSpec, SavedQuery};

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

#[test]
fn a_new_note_and_its_body_share_one_durable_update() {
    let fixture = tempfile::tempdir().unwrap();
    let home = fixture.path().join("Brain");
    let cache = fixture.path().join("cache");
    let mut brain = PagedBrain::open_library(&home, &cache).unwrap();
    let note = brain
        .create_note_with_text(Place::default(), "A complete note")
        .unwrap();
    assert_eq!(
        std::fs::read_dir(home.join("store/journal"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        brain.note(note).unwrap().as_deref(),
        Some("A complete note")
    );
    drop(brain);
    std::fs::remove_file(cache.join("live.sqlite")).unwrap();
    let mut reopened = PagedBrain::open_library(&home, &cache).unwrap();
    assert_eq!(
        reopened.note(note).unwrap().as_deref(),
        Some("A complete note")
    );
    assert!(reopened.set_note(note, "Revised").unwrap());
}

#[test]
fn indexed_queries_follow_edits_and_rebuild_after_cache_loss() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("Brain");
    let cache = home.path().join("cache");
    let mut brain = PagedBrain::open_library(&root, &cache).unwrap();
    let tag = NodeId::Builtin(Builtin::Book);
    let year = FieldId::Builtin {
        tag: Builtin::Book,
        slot: 1,
    };
    let page = brain
        .create_note_with_text(Place::default(), "City maps")
        .unwrap();
    let note = brain
        .create_note_with_text(
            Place {
                parent: Some(page),
                after: None,
            },
            "Historical maps",
        )
        .unwrap();
    let query = SavedQuery::Text("Historical maps".into());
    brain.set_tag(note, tag, true).unwrap();
    brain.set_link(note, page, true).unwrap();
    brain.set_field(note, year, Some("2026".into())).unwrap();
    brain.set_search(page, Some(query.clone())).unwrap();
    brain
        .define_field(
            tag,
            Some(year),
            FieldSpec {
                name: "Date of printing".into(),
                kind: FieldKind::Date,
                required: false,
            },
        )
        .unwrap();
    let ids = |matches: Vec<grimoire_brain::bounded_store::OutlineRow>| {
        matches.into_iter().map(|row| row.id).collect::<Vec<_>>()
    };
    assert_eq!(ids(brain.search(&query, 0, 20).unwrap()), [note]);
    assert_eq!(
        ids(brain.search(&SavedQuery::Tagged(tag), 0, 20).unwrap()),
        [note]
    );
    assert_eq!(
        ids(brain.search(&SavedQuery::LinksTo(page), 0, 20).unwrap()),
        [note]
    );
    assert!(
        brain
            .set_field(note, year, Some("not a date".into()))
            .is_err()
    );
    assert_eq!(brain.details(note).unwrap().fields, [(year, "2026".into())]);

    brain.set_note(note, "Modern maps").unwrap();
    assert!(brain.search(&query, 0, 20).unwrap().is_empty());
    drop(brain);
    std::fs::remove_file(cache.join("live.sqlite")).unwrap();
    let mut brain = PagedBrain::open_library(&root, &cache).unwrap();
    assert_eq!(brain.details(page).unwrap().query, Some(query));
    assert_eq!(
        ids(brain
            .search(&SavedQuery::Text("Modern maps".into()), 0, 20)
            .unwrap()),
        [note]
    );
    assert_eq!(
        ids(brain
            .search(
                &SavedQuery::Field {
                    field: year,
                    value: "2026".into()
                },
                0,
                20
            )
            .unwrap()),
        [note]
    );
    assert_eq!(
        ids(brain.search(&SavedQuery::LinksTo(page), 0, 20).unwrap()),
        [note]
    );
}

#[test]
fn moving_a_subtree_updates_visible_order_and_refuses_cycles() {
    let root = tempfile::tempdir().unwrap();
    let mut brain = PagedBrain::open(root.path(), 41).unwrap();
    let a = brain.create_note_with_text(Place::default(), "A").unwrap();
    let b = brain.create_note_with_text(Place::default(), "B").unwrap();
    let child = brain
        .create_note_with_text(
            Place {
                parent: Some(b),
                after: None,
            },
            "Child",
        )
        .unwrap();
    brain
        .move_note(
            b,
            Place {
                parent: Some(a),
                after: None,
            },
        )
        .unwrap();
    assert!(
        brain
            .move_note(
                a,
                Place {
                    parent: Some(child),
                    after: None
                }
            )
            .is_err()
    );
    assert_eq!(brain.children(None, 0, 10).unwrap()[0].id, a);
    assert_eq!(brain.children(Some(a), 0, 10).unwrap()[0].id, b);
    assert_eq!(brain.children(Some(b), 0, 10).unwrap()[0].id, child);
    assert_eq!(brain.project().unwrap().children(Some(a)), [b]);
}

#[test]
fn restart_refreshes_stale_markdown_and_moves_linked_anchors() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("Brain");
    let cache = fixture.path().join("cache");
    let copies = root.join("readable");
    let mut brain = PagedBrain::open_library(&root, &cache).unwrap();
    let a = brain
        .create_note_with_text(Place::default(), "Maps")
        .unwrap();
    let b = brain
        .create_note_with_text(Place::default(), "Drawings")
        .unwrap();
    let child = brain
        .create_note_with_text(
            Place {
                parent: Some(a),
                after: None,
            },
            "A sketch",
        )
        .unwrap();
    brain.set_link(child, b, true).unwrap();
    let page = copies.join(format!("{}.md", a.external_id()));
    let wait_for = |brain: &PagedBrain, path: &std::path::Path, text: &str| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !std::fs::read_to_string(path).is_ok_and(|page| page.contains(text)) {
            assert!(
                std::time::Instant::now() < deadline,
                "Missing {text} in {path:?}: {:?}. Worker reported {:?}",
                std::fs::read_to_string(path),
                brain.readable_error()
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    wait_for(&brain, &page, &format!("{}.md#", b.external_id()));
    brain
        .move_note(
            child,
            Place {
                parent: Some(b),
                after: None,
            },
        )
        .unwrap();
    let moved = copies.join(format!("{}.md", b.external_id()));
    wait_for(&brain, &moved, "A sketch");
    drop(brain);
    std::fs::remove_file(&moved).unwrap();
    std::fs::remove_file(cache.join("live.sqlite")).unwrap();
    let mut brain = PagedBrain::open_library(&root, &cache).unwrap();
    assert_eq!(brain.note(child).unwrap().as_deref(), Some("A sketch"));
    wait_for(&brain, &moved, "A sketch");
    wait_for(
        &brain,
        &copies.join(".version"),
        "70616765642d6f75746c696e65",
    );
}
