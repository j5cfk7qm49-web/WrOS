//! Shared document identity must survive missing/stale metadata without reuse.
use grimoire_core::{
    continuity::{Certainty, ContinuityStore, Fact, FactCategory},
    milestones::{MilestoneStore, VariantStore},
    review_notes::{PATH, ReviewNotes},
};
use std::{collections::HashMap, fs, path::PathBuf};
fn root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "grimoire-story-identity-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("novel.toml"), "title = \"Identity fixture\"\n").unwrap();
    fs::create_dir_all(root.join("manuscript/01-Chapter")).unwrap();
    fs::write(
        root.join("manuscript/01-Chapter/01-A.md"),
        "Original scene A.\n",
    )
    .unwrap();
    fs::write(
        root.join("manuscript/01-Chapter/02-B.md"),
        "Independent scene B.\n",
    )
    .unwrap();
    root
}
#[test]
fn missing_or_empty_registry_never_reassigns_historical_milestone_or_variant_ids() {
    for kind in ["milestone", "variant", "fact", "event"] {
        let root = root(kind);
        let mut registry = ReviewNotes::load(&root).unwrap();
        let id = registry
            .update(|s| s.register("manuscript/01-Chapter/01-A.md"))
            .unwrap();
        let original = fs::read(root.join(PATH)).unwrap();
        match kind {
            "milestone" => {
                MilestoneStore::load(&root)
                    .unwrap()
                    .update(|s| {
                        s.create_milestone(
                            "Original",
                            "",
                            HashMap::from([(id, "Original scene A.\n".into())]),
                        )
                    })
                    .unwrap();
            }
            "variant" => {
                VariantStore::load(&root)
                    .unwrap()
                    .update(|s| s.save_variant(id, "Original", "Original scene A.\n"))
                    .unwrap();
            }
            "fact" => {
                let mut fact = Fact::new(
                    FactCategory::Character,
                    "Wren",
                    "eyes",
                    "blue",
                    Certainty::Definite,
                );
                fact.scene_id = Some(id);
                ContinuityStore::load(&root)
                    .unwrap()
                    .update(|s| s.add_fact(fact))
                    .unwrap();
            }
            _ => {
                ContinuityStore::load(&root)
                    .unwrap()
                    .update(|s| {
                        s.add_event(1, "Arrival", "", Some(id), vec![], "Original chronology")
                    })
                    .unwrap();
            }
        }
        fs::rename(
            root.join(PATH),
            root.join(".grimoire/writingdeck/retained-original-registry.json"),
        )
        .unwrap();
        assert!(
            ReviewNotes::load(&root)
                .err()
                .unwrap()
                .to_string()
                .contains("missing/incomplete"),
            "{kind}"
        );
        assert!(!root.join(PATH).exists());
        let empty = serde_json::to_vec(&ReviewNotes::default()).unwrap();
        fs::write(root.join(PATH), &empty).unwrap();
        assert!(ReviewNotes::load(&root).is_err(), "{kind}");
        assert_eq!(fs::read(root.join(PATH)).unwrap(), empty);
        assert!(
            grimoire_core::project::rename(&root.join("manuscript/01-Chapter/02-B.md"), "Refused")
                .is_err()
        );
        assert!(root.join("manuscript/01-Chapter/02-B.md").is_file());
        fs::write(root.join(PATH), &original).unwrap();
        let restored = ReviewNotes::load(&root).unwrap();
        assert_eq!(
            restored.value.document(id).unwrap().path,
            "manuscript/01-Chapter/01-A.md"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn newly_arrived_orphan_references_block_allocation_before_the_callback_runs() {
    let root = root("cached");
    let mut registry = ReviewNotes::load(&root).unwrap();
    MilestoneStore::load(&root)
        .unwrap()
        .update(|s| {
            s.create_milestone(
                "External original",
                "",
                HashMap::from([(1, "Scene A original.\n".into())]),
            )
        })
        .unwrap();
    let called = std::cell::Cell::new(false);
    assert!(
        registry
            .update(|s| {
                called.set(true);
                s.register("manuscript/01-Chapter/02-B.md")
            })
            .is_err()
    );
    assert!(!called.get());
    assert!(registry.value.documents.is_empty());
    assert!(!root.join(PATH).exists());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn bookwide_facts_without_document_references_allow_a_fresh_registry() {
    let root = root("bookwide");
    let fact = Fact::new(
        FactCategory::Lore,
        "World",
        "sky",
        "blue",
        Certainty::Definite,
    );
    ContinuityStore::load(&root)
        .unwrap()
        .update(|s| s.add_fact(fact))
        .unwrap();
    let mut registry = ReviewNotes::load(&root).unwrap();
    let id = registry
        .update(|s| s.register("manuscript/01-Chapter/01-A.md"))
        .unwrap();
    assert_eq!(id, 1);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn unknown_future_and_oversized_dependencies_are_preserved_and_cannot_publish_a_registry() {
    for bytes in [
        b"unknown original story data".to_vec(),
        br#"{"schema_version":9,"next_id":0,"variants":[]}"#.to_vec(),
        vec![b'x'; 1024 * 1024 + 1],
    ] {
        let root = root("unknown");
        let path = root.join(".grimoire/writingdeck/variants.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(ReviewNotes::load(&root).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!root.join(PATH).exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn pending_move_cannot_gain_a_competing_identity_before_explicit_reconnection() {
    let root = root("pending-move");
    let mut registry = ReviewNotes::load(&root).unwrap();
    let id = registry
        .update(|s| s.register("manuscript/01-Chapter/01-A.md"))
        .unwrap();
    registry
        .update(|s| {
            s.documents[0].needs_reconnect = true;
            Ok(())
        })
        .unwrap();
    fs::rename(
        root.join("manuscript/01-Chapter/01-A.md"),
        root.join("manuscript/01-Chapter/03-Moved-A.md"),
    )
    .unwrap();
    let original = fs::read(root.join(PATH)).unwrap();
    assert!(
        registry
            .update(|s| s.register("manuscript/01-Chapter/03-Moved-A.md"))
            .is_err()
    );
    assert!(
        registry
            .update(|s| s.register("manuscript/01-Chapter/02-B.md"))
            .is_err()
    );
    assert_eq!(registry.value.documents.len(), 1);
    assert_eq!(fs::read(root.join(PATH)).unwrap(), original);
    registry
        .update(|s| s.relocate(id, "manuscript/01-Chapter/03-Moved-A.md"))
        .unwrap();
    assert_eq!(
        registry
            .update(|s| s.register("manuscript/01-Chapter/02-B.md"))
            .unwrap(),
        2
    );
    assert_eq!(
        registry.value.document(id).unwrap().path,
        "manuscript/01-Chapter/03-Moved-A.md"
    );
    fs::remove_dir_all(root).unwrap();
}
