use super::*;
use app::{continuity::Page as CPage, milestones::Page as MPage};
use grimoire_core::{
    milestones::{MilestoneStore, VariantStore},
    review_notes::ReviewNotes,
};

#[test]
fn argon_variant_restore_and_chapter_queue_use_original_source_and_existing_voice() {
    let mut d = writer("argon-variant", 100, 30);
    let original = d.app.editor.text();
    d.app.run_action(palette::Action::Milestones);
    d.key(KeyCode::Char('2'));
    d.key(KeyCode::Char('n'));
    d.app.paste("Alternate λ");
    d.key(KeyCode::Enter);
    d.key(KeyCode::Esc);
    d.app.editor.set_text("Current variation.");
    d.app.save();
    d.app.run_action(palette::Action::Milestones);
    d.key(KeyCode::Char('2'));
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('r'));
    assert_eq!(d.app.editor.text(), original);
    d.ctrl('z');
    assert_eq!(d.app.editor.text(), "Current variation.");

    let root = book("argon-reading-queue", true);
    let first = first_scene(&root);
    fs::write(&first, "The first original sentence.\n").unwrap();
    let second = first.parent().unwrap().join("02-Next.md");
    fs::write(&second, "The second original sentence.\n").unwrap();
    let mut reader = Desk::open(root, 100, 30);
    reader.app.run_action(palette::Action::Open(first.clone()));
    reader.app.run_action(palette::Action::ReadChapterAloud);
    assert_eq!(reader.app.reading.as_ref().unwrap().queue.len(), 1);
    assert_eq!(reader.app.reading.as_ref().unwrap().queue[0].0, second);
    reader.key(KeyCode::Char('m'));
    assert_eq!(
        ReviewNotes::load(&reader.root).unwrap().value.notes.len(),
        1
    );
    reader.key(KeyCode::Char('n'));
    assert_eq!(
        reader.app.project.nodes[reader.app.open.unwrap()].path,
        second
    );
    reader.key(KeyCode::Esc);
    assert_eq!(
        fs::read_to_string(&first).unwrap(),
        "The first original sentence.\n"
    );
    reader.app.run_action(palette::Action::Open(first.clone()));
    reader.app.run_action(palette::Action::ReadChapterAloud);
    fs::write(&second, "External changed original.\n").unwrap();
    reader.key(KeyCode::Char('n'));
    assert!(reader.app.reading.is_none());
    assert_eq!(
        reader.app.project.nodes[reader.app.open.unwrap()].path,
        first
    );
}

fn save_named_draft(d: &mut Desk, variant: bool, name: &str) {
    d.app.run_action(palette::Action::Milestones);
    if variant {
        d.key(KeyCode::Char('2'));
    }
    d.key(KeyCode::Char('n'));
    d.app.paste(name);
    d.key(KeyCode::Enter);
    assert_eq!(
        d.app.milestones.nav.current(),
        if variant {
            MPage::Variants
        } else {
            MPage::Milestones
        },
        "{}",
        d.app.milestones.notice
    );
    d.key(KeyCode::Esc);
}

#[test]
fn restoring_a_second_milestone_or_variant_uses_exactly_the_reviewed_snapshot_at_four_sizes() {
    for variant in [false, true] {
        for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
            let mut d = writer(&format!("reviewed-choice-{variant}-{width}"), width, height);
            for (name, text) in [
                ("First saved", "λ First saved body.\n"),
                ("Second saved", "λ Second saved body.\n"),
            ] {
                d.app.editor.set_text(text);
                d.app.save();
                save_named_draft(&mut d, variant, name);
            }
            let current = "λ Current body. %% untouched original %%\n";
            d.app.editor.set_text(current);
            d.app.save();
            d.app.run_action(palette::Action::Milestones);
            if variant {
                d.key(KeyCode::Char('2'));
            }
            d.key(KeyCode::Down);
            d.key(KeyCode::Enter);
            assert!(d.shows("Second saved"));
            d.key(KeyCode::Char('r'));
            assert_eq!(
                d.app.editor.text(),
                "λ Second saved body.\n",
                "{}",
                d.app.milestones.notice
            );
            assert_eq!(d.app.overlay, Overlay::None);
            d.ctrl('z');
            assert_eq!(d.app.editor.text(), current);
            let recovery = VariantStore::load(&d.root).unwrap();
            assert_eq!(recovery.value.variants.last().unwrap().content, current);
            let path = &d.app.project.nodes[d.app.open.unwrap()].path;
            assert!(
                grimoire_core::history::versions(&d.root, path)
                    .iter()
                    .any(|v| v.body() == current)
            );
        }
    }
}

#[test]
fn passage_review_defaults_cancel_then_restores_only_selected_wording_with_undo_at_four_sizes() {
    for variant in [false, true] {
        for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
            let mut d = writer(&format!("passage-{variant}-{width}"), width, height);
            let saved = "λ red retained middle words blue end. %% private %%\n";
            let current = "λ green retained middle words gold end. %% private %%\n";
            d.app.editor.set_text(saved);
            d.app.save();
            save_named_draft(&mut d, variant, "Passage saved");
            d.app.editor.set_text(current);
            d.app.save();
            d.app.run_action(palette::Action::Milestones);
            if variant {
                d.key(KeyCode::Char('2'));
            }
            d.key(KeyCode::Enter);
            d.key(KeyCode::Char('p'));
            assert_eq!(d.app.milestones.nav.current(), MPage::ReviewPassage);
            assert_eq!(d.app.milestones.nav.selected(), 0);
            assert!(d.shows("CURRENT WORDING"));
            d.app.paste("blocked hidden prose");
            d.ctrl('x');
            d.ctrl('z');
            d.key(KeyCode::F(1));
            assert!(d.shows("Continuity & draft milestones"));
            d.key(KeyCode::Esc);
            d.key(KeyCode::Enter);
            assert_eq!(d.app.editor.text(), current);
            assert_ne!(d.app.milestones.nav.current(), MPage::ReviewPassage);
            d.key(KeyCode::Right);
            d.key(KeyCode::Char('p'));
            assert!(d.shows("gold"));
            assert!(d.shows("blue"));
            d.key(KeyCode::Down);
            d.key(KeyCode::Enter);
            assert_eq!(d.app.overlay, Overlay::None, "{}", d.app.milestones.notice);
            assert_eq!(
                d.app.editor.text(),
                "λ green retained middle words blue end. %% private %%\n"
            );
            assert_eq!(
                VariantStore::load(&d.root)
                    .unwrap()
                    .value
                    .variants
                    .last()
                    .unwrap()
                    .content,
                current
            );
            d.ctrl('z');
            assert_eq!(d.app.editor.text(), current);
        }
    }
}

#[test]
fn passage_restore_refuses_changed_revision_metadata_and_backing_without_losing_source() {
    for fault in ["revision", "metadata", "in-process metadata", "backing"] {
        let mut d = writer(&format!("passage-refusal-{fault}"), 100, 30);
        milestone(&mut d);
        d.key(KeyCode::Esc);
        d.app.editor.set_text("Current λ original.\n");
        d.app.save();
        d.app.run_action(palette::Action::Milestones);
        d.key(KeyCode::Enter);
        d.key(KeyCode::Char('p'));
        d.key(KeyCode::Down);
        let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
        let metadata = d.root.join(".grimoire/writingdeck/variants.json");
        match fault {
            "revision" => d.app.editor.set_text("Changed after review.\n"),
            "metadata" => fs::write(&metadata, b"unknown external metadata").unwrap(),
            "in-process metadata" => {
                let id = *d.app.milestones.store.as_ref().unwrap().value.milestones[0]
                    .scene_snapshots
                    .keys()
                    .next()
                    .unwrap();
                d.app
                    .milestones
                    .variants
                    .as_mut()
                    .unwrap()
                    .update(|v| {
                        v.save_variant(id, "Another variant", "Independent saved text")?;
                        Ok(())
                    })
                    .unwrap();
            }
            "backing" => fs::write(&path, "External λ original.\n").unwrap(),
            _ => unreachable!(),
        }
        let before = d.app.editor.text();
        let disk = fs::read(&path).unwrap();
        let meta = fs::read(&metadata).ok();
        d.key(KeyCode::Enter);
        assert_eq!(d.app.editor.text(), before, "{fault}");
        assert_eq!(fs::read(&path).unwrap(), disk, "{fault}");
        assert_eq!(fs::read(&metadata).ok(), meta, "{fault}");
        assert_eq!(
            d.app.milestones.nav.current(),
            MPage::ReviewPassage,
            "{fault}"
        );
    }
}

#[test]
fn passage_history_write_failure_retains_prose_and_the_recovery_variant() {
    let mut d = writer("passage-history-failure", 100, 30);
    milestone(&mut d);
    d.key(KeyCode::Esc);
    let current = "λ Current wording remains. %% private original %%\n";
    d.app.editor.set_text(current);
    d.app.save();
    d.app.run_action(palette::Action::Milestones);
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Down);
    let history = d.root.join(".grimoire/history");
    let retained = d.root.join(".grimoire/retained-history");
    if history.exists() {
        fs::rename(&history, &retained).unwrap();
    }
    fs::write(&history, "Original fixture obstruction.\n").unwrap();
    let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    let disk = fs::read(&path).unwrap();
    d.key(KeyCode::Enter);
    assert_eq!(d.app.editor.text(), current);
    assert_eq!(fs::read(&path).unwrap(), disk);
    assert_eq!(
        fs::read_to_string(&history).unwrap(),
        "Original fixture obstruction.\n"
    );
    let recovery = VariantStore::load(&d.root).unwrap();
    assert_eq!(recovery.value.variants.last().unwrap().content, current);
    assert_eq!(d.app.milestones.nav.current(), MPage::ReviewPassage);
    let saved = fs::read(d.root.join(".grimoire/writingdeck/variants.json")).unwrap();
    d.key(KeyCode::Enter);
    assert!(d.app.milestones.notice.contains("metadata changed"));
    assert_eq!(
        fs::read(d.root.join(".grimoire/writingdeck/variants.json")).unwrap(),
        saved
    );
    assert_eq!(d.app.editor.text(), current);
}

fn writer(tag: &str, w: u16, h: u16) -> Desk {
    let mut d = metric_writer(tag, w, h);
    d.app
        .editor
        .set_text("λ Original wording. %% private note %%\nA second sentence.");
    d.app.save();
    d
}
fn milestone(d: &mut Desk) {
    d.app.run_action(palette::Action::Milestones);
    d.key(KeyCode::Char('n'));
    d.app.paste("Draft λ");
    d.key(KeyCode::Enter);
    assert_eq!(
        d.app.milestones.nav.current(),
        MPage::Milestones,
        "{}",
        d.app.milestones.notice
    );
    assert_eq!(
        d.app
            .milestones
            .store
            .as_ref()
            .unwrap()
            .value
            .milestones
            .len(),
        1
    );
}
#[test]
fn argon_milestone_comparison_restore_undo_and_durable_identity_at_four_sizes() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = writer(&format!("argon-milestone-{w}"), w, h);
        let original = d.app.editor.text();
        let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
        milestone(&mut d);
        let documents = ReviewNotes::load(&d.root).unwrap().value.documents;
        let id = documents
            .iter()
            .find(|x| d.root.join(&x.path) == path)
            .unwrap()
            .id;
        assert!(
            MilestoneStore::load(&d.root).unwrap().value.milestones[0]
                .scene_snapshots
                .contains_key(&id)
        );
        d.key(KeyCode::Esc);
        d.app.editor.set_text("Revised current wording.");
        d.app.save();
        // Inserting an earlier scene changes tree indices across reload.
        fs::write(
            path.parent().unwrap().join("00-New.md"),
            "An independent new scene.",
        )
        .unwrap();
        let mut reopened = Desk::open(d.root.clone(), w, h);
        reopened.app.run_action(palette::Action::Open(path.clone()));
        reopened.app.run_action(palette::Action::Milestones);
        reopened.key(KeyCode::Enter);
        assert_eq!(
            reopened.app.milestones.nav.current(),
            MPage::DiffMilestone,
            "{}",
            reopened.app.milestones.notice
        );
        assert!(reopened.shows("Scene Comparison"));
        reopened.key(KeyCode::Enter);
        assert_eq!(
            reopened.app.editor.text(),
            "Revised current wording.\n",
            "Enter only inspects the comparison"
        );
        reopened.key(KeyCode::Char('r'));
        assert_eq!(
            reopened.app.overlay,
            Overlay::None,
            "{}",
            reopened.app.milestones.notice
        );
        assert_eq!(reopened.app.editor.text(), original);
        reopened.ctrl('z');
        assert_eq!(reopened.app.editor.text(), "Revised current wording.\n");
        assert!(!grimoire_core::history::versions(&reopened.root, &path).is_empty());
    }
}
#[test]
fn argon_restore_refuses_changed_revision_or_metadata_and_keeps_original() {
    let mut d = writer("argon-restore-refusal", 100, 30);
    milestone(&mut d);
    d.key(KeyCode::Esc);
    d.app.editor.set_text("Current preserved.");
    d.app.save();
    d.app.run_action(palette::Action::Milestones);
    d.key(KeyCode::Enter);
    d.app.editor.set_text("Changed after comparison.");
    d.key(KeyCode::Char('r'));
    assert_eq!(d.app.editor.text(), "Changed after comparison.");
    assert!(d.app.milestones.notice.contains("revision changed"));
    d.key(KeyCode::Esc);
    d.key(KeyCode::Enter);
    let metadata = d.root.join(".grimoire/writingdeck/variants.json");
    fs::write(&metadata, b"external original metadata").unwrap();
    d.key(KeyCode::Char('r'));
    assert_eq!(d.app.editor.text(), "Changed after comparison.");
    assert_eq!(fs::read(metadata).unwrap(), b"external original metadata");
    assert_ne!(d.app.overlay, Overlay::None);
}
#[test]
fn argon_continuity_fact_timeline_thread_flow_preserves_editor_at_four_sizes() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = writer(&format!("argon-continuity-{w}"), w, h);
        fs::write(
            d.root.join(".writingdeck-demo"),
            "WritingDeck safe tutorial project v1\n",
        )
        .unwrap();
        d.app.deck.progress.start(31).unwrap();
        let before = d.app.editor.text();
        d.app.run_action(palette::Action::Continuity);
        d.key(KeyCode::Char('4'));
        assert!(!d.app.deck.progress.completed.contains(&31));
        for value in ["blue", "green"] {
            d.key(KeyCode::Char('1'));
            d.key(KeyCode::Char('n'));
            for text in ["Wren", "eyes", value, "canon"] {
                d.app.paste(text);
                d.key(KeyCode::Tab);
            }
            d.key(KeyCode::Enter);
            assert_eq!(
                d.app.continuity.nav.current(),
                CPage::Facts,
                "{}",
                d.app.continuity.notice
            );
        }
        d.key(KeyCode::Char('4'));
        assert!(d.app.deck.progress.completed.contains(&31));
        assert_eq!(d.app.continuity.conflicts.len(), 1);
        assert!(d.shows("Wren"));
        d.key(KeyCode::Char('2'));
        d.key(KeyCode::Char('n'));
        d.app.paste("1");
        d.key(KeyCode::Tab);
        d.app.paste("Departure");
        d.key(KeyCode::Enter);
        assert_eq!(
            d.app.continuity.store.as_ref().unwrap().value.events.len(),
            1
        );
        d.key(KeyCode::Char('3'));
        d.key(KeyCode::Char('n'));
        d.app.paste("Find the key");
        d.key(KeyCode::Enter);
        assert_eq!(
            d.app.continuity.store.as_ref().unwrap().value.threads.len(),
            1
        );
        d.app.paste("unrouted");
        d.ctrl('x');
        d.ctrl('z');
        d.key(KeyCode::Esc);
        assert_eq!(d.app.editor.text(), before);
        let loaded = grimoire_core::continuity::ContinuityStore::load(&d.root).unwrap();
        assert_eq!(loaded.value.facts.len(), 2);
        assert!(loaded.value.facts[0].scene_id.is_some());
    }
}
