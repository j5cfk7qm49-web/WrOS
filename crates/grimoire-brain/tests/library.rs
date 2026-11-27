#![cfg(test)]
#![cfg(feature = "runtime")]

use grimoire_brain::bounded_store::PagedBrain;
use grimoire_brain::library::{Edition, Filter, Sort};
use grimoire_brain::model::{Builtin, FieldId, NodeId, Place};
use serde_json::json;

fn edition(title: &str, author: &str, year: i64, kind: &str) -> Edition {
    Edition::new(
        json!({
            "id": "imported",
            "title": title,
            "type": kind,
            "author": [{"family": author}],
            "issued": {"date-parts": [[year]]},
        })
        .as_object()
        .unwrap()
        .clone(),
        1234,
    )
}

#[test]
fn works_filter_and_sort_after_rebuilding_the_cache() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = PagedBrain::open(temp.path(), 7).unwrap();
    let book = store
        .create_work(
            Place::default(),
            edition("Journal of Roads", "Zhang", 2024, "book"),
        )
        .unwrap();
    let article = store
        .create_work(
            Place::default(),
            edition("Behind the Library", "Abbott", 2020, "article-journal"),
        )
        .unwrap();
    let tag = NodeId::Builtin(Builtin::Concept);
    store.set_tag(book.work, tag, true).unwrap();
    store
        .set_field(
            book.work,
            FieldId::Builtin {
                tag: Builtin::Book,
                slot: 3,
            },
            Some("Read next".into()),
        )
        .unwrap();
    let second = store
        .add_edition(book.work, edition("Roads, revised", "Zhang", 2026, "book"))
        .unwrap();
    assert_ne!(book.edition, second.edition);
    assert_eq!(book.work, second.work);
    let tagged_only = Filter {
        tag: Some(tag),
        ..Filter::default()
    };
    assert_eq!(
        store.library_works(&tagged_only, 0, 32).unwrap()[0].id,
        book.work
    );
    let geography = Filter {
        text: "revised".into(),
        source_type: Some(Builtin::Book),
        ..Filter::default()
    };
    let status = Filter {
        text: "Read next".into(),
        sort: Sort::Status,
        ..Filter::default()
    };
    assert_eq!(
        store.library_works(&status, 0, 32).unwrap()[0].status,
        "Read next"
    );
    assert_eq!(
        store.library_works(&geography, 0, 32).unwrap()[0].editions,
        2
    );
    drop(store);

    std::fs::remove_file(temp.path().join("live.sqlite")).unwrap();
    let mut store = PagedBrain::open(temp.path(), 7).unwrap();
    let by_author = Filter {
        sort: Sort::Author,
        ..Filter::default()
    };
    assert_eq!(
        store.library_works(&by_author, 0, 32).unwrap()[0].id,
        article.work
    );
    let results = store.find_editions("Zhang", 32).unwrap();
    assert_eq!(results.len(), 2);
    let mut latest = store.edition(second).unwrap().unwrap();
    latest.label = "Second edition".into();
    store.save_edition(second, latest).unwrap();
    assert_eq!(
        store.edition(second).unwrap().unwrap().label,
        "Second edition"
    );
    assert_eq!(
        store.note(book.work).unwrap().as_deref(),
        Some("Journal of Roads")
    );
}

#[test]
fn identical_media_from_two_paths_shares_one_blob() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("Brain");
    let cache = temp.path().join("cache");
    let one = temp.path().join("one.pdf");
    let two = temp.path().join("two.pdf");
    let bytes = b"%PDF-1.7\nsmall fixture\n";
    std::fs::write(&one, bytes).unwrap();
    std::fs::write(&two, bytes).unwrap();
    let mut store = PagedBrain::open_library(&library, &cache).unwrap();
    let first = store
        .create_work(Place::default(), edition("Book", "Ng", 2020, "book"))
        .unwrap();
    let second = store
        .add_edition(first.work, edition("Book, revised", "Ng", 2022, "book"))
        .unwrap();
    let a = store.attach_media(first, &one).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::metadata(&library).unwrap().permissions();
        std::fs::set_permissions(&library, std::fs::Permissions::from_mode(0o500)).unwrap();
        let reused = grimoire_brain::library::import_media(&library, &two);
        std::fs::set_permissions(&library, permissions).unwrap();
        assert_eq!(reused.unwrap().sha256, a.sha256);
    }
    let b = store.attach_media(second, &two).unwrap();
    assert_eq!(a.sha256, b.sha256);
    assert_ne!(a.name, b.name);
    assert_eq!(std::fs::read_dir(library.join("files")).unwrap().count(), 1);
    let blob = library.join("files").join(&a.sha256);
    assert_eq!(std::fs::read(&blob).unwrap(), bytes);
    std::fs::write(&blob, vec![0_u8; bytes.len()]).unwrap();
    assert!(store.attach_media(first, &one).is_err());
}

#[test]
fn a_cited_note_keeps_its_text_links_and_cards_in_one_operation() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = PagedBrain::open(temp.path(), 7).unwrap();
    let cited = store
        .create_work(Place::default(), edition("Roads", "Zhang", 2024, "book"))
        .unwrap();
    let note = store
        .create_tagged_note_with_links(
            Place::default(),
            "Found it :: In Roads",
            None,
            [cited.work].into(),
        )
        .unwrap();
    let next = store.create_note(Place::default()).unwrap();
    let (NodeId::Note(note_id), NodeId::Note(next_id)) = (note, next) else {
        panic!("expected journal-backed notes");
    };
    assert_eq!(next_id.clock, note_id.clock + 1);
    assert_eq!(store.details(note).unwrap().links, vec![cited.work]);
    assert_eq!(store.due_cards(0, None).unwrap().count, 1);
    assert_eq!(store.project().unwrap().links(note), vec![cited.work]);
    drop(store);
    // A pre-RC28 build can read the text and cards. Its cache lacks the links.
    let cache = rusqlite::Connection::open(temp.path().join("live.sqlite")).unwrap();
    let mut stamp: Vec<u8> = cache
        .query_row("SELECT stamp FROM view_state", [], |row| row.get(0))
        .unwrap();
    assert!(stamp.starts_with(b"paged-outline-v6\0"));
    stamp[b"paged-outline-v".len()] = b'5';
    cache
        .execute("UPDATE view_state SET stamp=?1", [&stamp])
        .unwrap();
    cache.execute("DELETE FROM note_links", []).unwrap();
    drop(cache);
    let mut rebuilt = PagedBrain::open(temp.path(), 7).unwrap();
    assert_eq!(
        rebuilt.note(note).unwrap().as_deref(),
        Some("Found it :: In Roads")
    );
    assert_eq!(rebuilt.details(note).unwrap().links, vec![cited.work]);
    assert_eq!(
        rebuilt.due_cards(0, None).unwrap().next.unwrap().answer,
        "In Roads"
    );
}
