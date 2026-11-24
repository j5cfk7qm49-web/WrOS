use super::*;

#[test]
fn citation_picker_obeys_word_boundaries_and_inserts_one_undo_step() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = metric_writer(&format!("citation-{w}"), w, h);
        fs::write(
            d.root.join("sources.json"),
            r#"[{"id":"baron","type":"book","title":"Reading","author":[{"family":"Baron"}]}]"#,
        )
        .unwrap();
        d.typed("name@example.test @@ ");
        assert_eq!(d.app.editor.text(), "name@example.test @ ");
        d.typed("@Baron");
        assert!(d.shows("Cite a source"));
        d.key(KeyCode::Esc);
        assert_eq!(d.app.editor.text(), "name@example.test @ ");
        d.typed("@Baron");
        d.key(KeyCode::Enter);
        d.typed("194");
        d.key(KeyCode::Tab);
        d.key(KeyCode::Char(' '));
        d.key(KeyCode::Enter);
        assert_eq!(d.app.editor.text(), "name@example.test @ [-@baron, p. 194]");
        d.ctrl('z');
        assert_eq!(d.app.editor.text(), "name@example.test @ ");
    }
}

#[test]
fn citation_picker_keeps_an_accepted_library_edition_until_export_review() {
    use grimoire_brain::bounded_store::PagedBrain;
    use grimoire_brain::library::Edition;
    use grimoire_brain::model::Place;
    use grimoire_core::sources;
    use serde_json::json;

    let fixture = tempfile::tempdir().unwrap();
    let library = fixture.path().join("Brain");
    let cache = fixture.path().join("cache");
    {
        let mut store = PagedBrain::open_library(&library, &cache).unwrap();
        let record = json!({"id":"atlas","type":"book","title":"The Harbor Atlas",
            "author":[{"family":"Lin"}]});
        store
            .create_work(
                Place::default(),
                Edition::new(record.as_object().unwrap().clone(), 1234),
            )
            .unwrap();
    }
    let mut desk = metric_writer("citation-library-edition", 100, 30);
    desk.app.brain.set_paths(library, cache);
    desk.typed("@Harbor");
    assert!(desk.shows("The Harbor Atlas"));
    desk.key(KeyCode::Enter);
    desk.typed("18");
    desk.key(KeyCode::Enter);
    let manuscript = desk.app.editor.text();
    let project = sources::Snapshot::open(&desk.root).unwrap();
    let (key, record) = project.sources.iter().next().unwrap();
    assert_eq!(record["title"], "The Harbor Atlas");
    assert!(sources::library_provenance(record).is_some());
    assert_eq!(manuscript, format!("[@{key}, p. 18]"));

    desk.press(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::SUPER));
    desk.key(KeyCode::Enter);
    desk.key(KeyCode::Char('e'));
    desk.typed("title = The Updated Harbor Atlas");
    desk.key(KeyCode::Enter);
    desk.press(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::SUPER));
    desk.app.open_export();
    desk.draw();
    assert!(desk.shows("Details changed. Update?"));
    assert!(desk.shows("The Updated Harbor Atlas"));
    desk.key(KeyCode::Esc);
    assert!(matches!(desk.app.overlay, Overlay::Export { .. }));
    assert_eq!(
        sources::Snapshot::open(&desk.root)
            .unwrap()
            .sources
            .get(key),
        Some(record)
    );

    desk.app.open_export();
    desk.draw();
    desk.key(KeyCode::Enter);
    assert!(matches!(desk.app.overlay, Overlay::Export { .. }));
    let updated = sources::Snapshot::open(&desk.root).unwrap();
    assert_eq!(
        updated.sources.get(key).unwrap()["title"],
        "The Updated Harbor Atlas"
    );
    assert_eq!(desk.app.editor.text(), manuscript);
}

#[test]
fn insertion_refuses_external_pending_parked_hidden_and_aliased_sources_without_editing() {
    let mut d = metric_writer("footnote-external", 80, 24);
    d.app.editor.set_text("Original λ.");
    d.app.flush();
    d.app.save();
    let i = d.app.open.unwrap();
    let path = d.app.project.nodes[i].path.clone();
    let before = d.app.editor.text();
    fs::write(&path, "External source").unwrap();
    d.app.run_action(palette::Action::InsertFootnote);
    assert_eq!(d.app.editor.text(), before);
    assert!(d.app.msg.contains("backing source changed"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "External source");
    d.app.project.nodes[i].seen = grimoire_core::sync::Fingerprint::read(&path);
    d.app.project.nodes[i].parked = true;
    d.app.run_action(palette::Action::InsertFootnote);
    assert_eq!(d.app.editor.text(), before);
    assert!(d.app.msg.contains("parked"));
    d.app.project.nodes[i].parked = false;
    d.app.project.nodes[i].disk.unavailable = Some(project::Unavailable::empty());
    d.app.run_action(palette::Action::InsertFootnote);
    assert_eq!(d.app.editor.text(), before);
    assert!(d.app.msg.contains("disk change"));

    let (mut d, secondary) = secondary_book("footnote-alias", 100, 30, false);
    let primary = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    d.app
        .run_action(palette::Action::EditSecondary(secondary.clone()));
    d.draw();
    d.app.editor.set_text("Secondary original.");
    d.app.flush();
    d.app.save();
    let before = d.app.editor.text();
    d.app.views.secondary_visible = false;
    d.app.insert_new_footnote();
    assert_eq!(d.app.editor.text(), before);
    assert!(d.app.msg.contains("hidden"));
    d.app.views.secondary_visible = true;
    #[cfg(unix)]
    {
        fs::remove_file(&secondary).unwrap();
        std::os::unix::fs::symlink(&primary, &secondary).unwrap();
        d.app.insert_new_footnote();
        assert_eq!(d.app.editor.text(), before);
        assert!(
            d.app.msg.contains("alias") || d.app.msg.contains("belongs"),
            "{}",
            d.app.msg
        );
    }
}

#[test]
fn secondary_footnote_keeps_original_primary_undo_and_does_not_award_typing_at_four_sizes() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut d, secondary) = secondary_book(&format!("footnote-second-{w}"), w, h, false);
        metric_clock(&mut d);
        d.app
            .run_action(palette::Action::EditSecondary(secondary.clone()));
        d.draw();
        d.app.editor.set_text("Café 🦊 original prose.");
        d.app.flush();
        d.app.save();
        d.app.editor.place(0, 6);
        let primary = d.app.view_position(grimoire_core::documents::Pane::Primary);
        let before = d.app.editor.text();
        let bytes = fs::read(&secondary).unwrap();
        d.app.capture_productivity();
        let totals = d.app.productivity.ledger.known.clone();
        d.app.run_action(palette::Action::InsertFootnote);
        assert!(d.app.editor.text().contains("🦊[^fn-1]"));
        d.app.capture_productivity();
        assert_eq!(d.app.productivity.ledger.known, totals);
        assert_eq!(
            d.app.view_position(grimoire_core::documents::Pane::Primary),
            primary
        );
        d.ctrl('z');
        assert_eq!(d.app.editor.text(), before);
        d.ctrl('y');
        assert!(d.app.editor.text().contains("[^fn-1]: "));
        d.app.paste("Original footnote λ.");
        d.ctrl('s');
        assert!(
            fs::read_to_string(&secondary)
                .unwrap()
                .contains("[^fn-1]: Original footnote λ.")
        );
        d.ctrl('z');
        d.ctrl('z');
        d.ctrl('s');
        assert_eq!(fs::read(&secondary).unwrap(), bytes);
    }
}
