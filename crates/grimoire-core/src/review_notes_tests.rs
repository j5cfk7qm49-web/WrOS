use super::*;
use std::{fs, path::PathBuf};
fn root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("wros-review-notes-{tag}-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("novel.toml"), "title = 'Original'\n").unwrap();
    root
}
fn add(store: &mut Store<ReviewNotes>, path: &str, text: &str) -> u64 {
    store
        .update(|s| {
            s.add(
                path,
                Anchor::capture(text, 0, text.len())?,
                "Original review λ".into(),
            )
        })
        .unwrap()
}
#[test]
fn exact_unicode_context_movement_and_ambiguity_use_original_byte_ranges() {
    let text = "İ café 🦊 e\u{301} — the silver key rests here.";
    let start = text.find("the silver").unwrap();
    let anchor = Anchor::capture(text, start, start + 14).unwrap();
    assert_eq!(
        &text[anchor.locate(text).unwrap().0..anchor.locate(text).unwrap().1],
        "the silver key"
    );
    let moved = format!("A new paragraph.\n{text}");
    assert_eq!(
        anchor.locate(&moved).unwrap().0,
        start + "A new paragraph.\n".len()
    );
    assert!(anchor.locate(&format!("{text}\n{text}")).is_none());
    assert!(anchor.locate(&text.replace("silver", "golden")).is_none());
    assert!(Anchor::capture(text, 1, 5).is_err());
    assert!(Anchor::capture("a".repeat(300).as_str(), 100, 101).is_err());
}
#[test]
fn document_ids_survive_folder_rename_swap_trash_restore_and_restart() {
    let root = root("moves");
    fs::create_dir_all(root.join("manuscript/chapter")).unwrap();
    let a = root.join("manuscript/chapter/01-A.md");
    let b = root.join("manuscript/chapter/02-B.md");
    let original = "---\ntitle: 'Author title'\ncustom: λ\n---\n\nOriginal 🦊.\n";
    fs::write(&a, original).unwrap();
    fs::write(&b, "Different source").unwrap();
    let mut store = ReviewNotes::load(&root).unwrap();
    add(&mut store, "manuscript/chapter/01-A.md", "Original 🦊.");
    add(&mut store, "manuscript/chapter/02-B.md", "Different source");
    crate::project::apply_moves(
        &root,
        &[(a.clone(), b.clone()), (b.clone(), a.clone())],
        false,
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&b).unwrap(), original);
    let value = ReviewNotes::load(&root).unwrap().value;
    assert_eq!(value.notes[0].current.document_id, 1);
    assert_eq!(
        value.document(1).unwrap().path,
        "manuscript/chapter/02-B.md"
    );
    assert_eq!(
        value.document(2).unwrap().path,
        "manuscript/chapter/01-A.md"
    );
    let folder = crate::project::rename(&root.join("manuscript/chapter"), "Renamed").unwrap();
    let scene = folder.join("02-B.md");
    let trashed = crate::project::trash(&root, &scene).unwrap();
    let restored = crate::project::restore(&root, &trashed).unwrap();
    let value = ReviewNotes::load(&root).unwrap().value;
    assert_eq!(root.join(&value.document(1).unwrap().path), restored);
    assert!(!value.document(1).unwrap().needs_reconnect);
    assert_eq!(fs::read_to_string(restored).unwrap(), original);
    assert_eq!(value.document(1).unwrap().previous_paths.len(), 4);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn prepared_or_failed_moves_are_durably_unavailable_not_rebound_by_path() {
    let root = root("interruption");
    let a = root.join("a.md");
    let b = root.join("b.md");
    fs::write(&a, "Original").unwrap();
    let mut store = ReviewNotes::load(&root).unwrap();
    add(&mut store, "a.md", "Original");
    let guard = MoveGuard::prepare(&[(a.clone(), b.clone())]).unwrap();
    drop(guard); // process death before filesystem move
    let mut recovered = ReviewNotes::load(&root).unwrap();
    assert!(recovered.value.document(1).unwrap().needs_reconnect);
    assert!(recovered.update(|s| s.register("a.md")).is_err());
    assert!(crate::project::rename(&a, "b").is_err());
    assert_eq!(fs::read_to_string(&a).unwrap(), "Original");
    recovered.update(|s| s.relocate(1, "a.md")).unwrap(); // explicit author reconnection
    let guard = MoveGuard::prepare(&[(a.clone(), b.clone())]).unwrap();
    fs::rename(a, b).unwrap();
    drop(guard); // process death after move but before identity commit
    let mut recovered = ReviewNotes::load(&root).unwrap();
    assert!(recovered.value.document(1).unwrap().needs_reconnect);
    recovered.update(|s| s.relocate(1, "b.md")).unwrap();
    assert_eq!(recovered.value.notes[0].current.document_id, 1);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn metadata_conflicts_schema_bounds_aliases_and_prototype_data_preserve_originals() {
    let root = root("preservation");
    let mut store = ReviewNotes::load(&root).unwrap();
    let id = add(&mut store, "a.md", "Original");
    let old = store.value.clone();
    assert!(
        store
            .update(|s| s.revise(id, |v| v.comment = String::new()))
            .is_err()
    );
    assert_eq!(store.value, old);
    let path = root.join(PATH);
    for bytes in [
        b"corrupt".to_vec(),
        b"{\"schema_version\":99}".to_vec(),
        vec![b' '; crate::statefile::MAX_BYTES + 1],
    ] {
        fs::write(&path, &bytes).unwrap();
        assert!(ReviewNotes::load(&root).is_err());
        assert!(store.update(|_| Ok(())).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::remove_file(&path).unwrap();
    #[cfg(unix)]
    {
        let original = root.join("original");
        fs::write(&original, "preserve").unwrap();
        std::os::unix::fs::symlink(&original, &path).unwrap();
        assert!(ReviewNotes::load(&root).is_err());
        assert_eq!(fs::read_to_string(original).unwrap(), "preserve");
        fs::remove_file(&path).unwrap();
    }
    fs::write(
        root.join(".grimoire/writingdeck/annotations.json"),
        "prototype",
    )
    .unwrap();
    assert!(ReviewNotes::load(&root).is_err());
    assert!(!path.exists());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn comment_and_anchor_versions_are_preserved_and_limits_refuse_discard() {
    let root = root("versions");
    let mut store = ReviewNotes::load(&root).unwrap();
    let id = add(&mut store, "a.md", "Original");
    let original = store.value.notes[0].current.clone();
    for n in 0..64 {
        store
            .update(|s| s.revise(id, |v| v.comment = format!("Revision {n}")))
            .unwrap();
    }
    let bytes = fs::read(root.join(PATH)).unwrap();
    assert_eq!(store.value.notes[0].previous[0], original);
    assert!(
        store
            .update(|s| s.revise(id, |v| v.comment = "one too many".into()))
            .is_err()
    );
    assert_eq!(fs::read(root.join(PATH)).unwrap(), bytes);
    assert!(
        store
            .update(|s| {
                s.documents.push(s.documents[0].clone());
                Ok(())
            })
            .is_err()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn document_locators_cannot_alias_an_existing_identity() {
    let mut notes = ReviewNotes::default();
    assert_eq!(notes.register("chapter/scene.md").unwrap(), 1);
    for path in [
        "chapter//scene.md",
        "chapter/./scene.md",
        "chapter/../scene.md",
        "./chapter/scene.md",
    ] {
        assert!(notes.register(path).is_err(), "{path}");
    }
    assert_eq!(notes.documents.len(), 1);
}
