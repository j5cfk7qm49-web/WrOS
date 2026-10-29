use super::*;
use app::review_notes::Page;
use grimoire_core::{
    annotations::AnnotationStatus as Status,
    review_notes::{self as model, ReviewNotes},
};

const SOURCE: &str =
    "İ café 🦊 e\u{301}. The silver key rests here.\nAnother original paragraph.\n";
fn writer(tag: &str, w: u16, h: u16) -> Desk {
    let mut d = metric_writer(tag, w, h);
    d.app.editor.set_text(SOURCE);
    d.app.flush();
    d.app.save();
    d.app.editor.select((0, 13), (0, 27)); // Unicode scalar columns, not bytes.
    d
}
fn create(d: &mut Desk) {
    d.app.run_action(palette::Action::ReviewNotes);
    d.key(KeyCode::Char('n'));
    assert_eq!(d.app.review_notes.nav.current(), Page::Comment);
    d.app.paste("Clarify the key's owner λ.");
    d.key(KeyCode::Enter);
    assert_eq!(
        d.app.review_notes.nav.current(),
        Page::Detail,
        "{}",
        d.app.review_notes.notice
    );
}
fn notes(d: &Desk) -> ReviewNotes {
    ReviewNotes::load(&d.root).unwrap().value
}

#[test]
fn annotation_creation_revision_status_reanchor_tutorial_and_undo_at_four_sizes() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = writer(&format!("review-notes-flow-{w}"), w, h);
        fs::write(
            d.root.join(".writingdeck-demo"),
            "WritingDeck safe tutorial project v1\n",
        )
        .unwrap();
        d.app.deck.progress.start(32).unwrap();
        let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
        let bytes = fs::read(&path).unwrap();
        let selection = d.app.editor.selection();
        let caret = (d.app.editor.cy, d.app.editor.cx, d.app.editor.scroll);
        create(&mut d);
        assert!(d.shows("Revision notes · Review"));
        assert!(d.shows("Exact unique passage available"));
        assert!(d.shows("g passage"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(d.app.editor.selection(), selection);
        assert_eq!(
            (d.app.editor.cy, d.app.editor.cx, d.app.editor.scroll),
            caret
        );
        d.app.paste("nxoesga");
        d.ctrl('x');
        d.ctrl('z');
        assert_eq!(d.app.editor.text(), SOURCE);
        d.key(KeyCode::Char('s'));
        assert_eq!(notes(&d).notes[0].current.status, Status::InProgress);
        d.key(KeyCode::Char('x'));
        assert_eq!(notes(&d).notes[0].current.status, Status::Resolved);
        assert!(!d.app.deck.progress.completed.contains(&32));
        d.key(KeyCode::Char('o'));
        d.key(KeyCode::Char('a')); // Same anchor cannot manufacture lesson progress.
        d.key(KeyCode::Down);
        d.key(KeyCode::Enter);
        assert_eq!(d.app.review_notes.nav.current(), Page::Reanchor);
        assert!(!d.app.deck.progress.completed.contains(&32));
        d.key(KeyCode::Esc);
        d.key(KeyCode::Char('g'));
        assert_eq!(d.app.overlay, Overlay::None);
        let original = d.app.editor.text();
        d.app.editor.replace_selection_with("the golden key");
        d.app.flush();
        d.app.save();
        d.app.editor.select((0, 13), (0, 27));
        d.app.run_action(palette::Action::ReviewNotes);
        d.key(KeyCode::Enter);
        d.key(KeyCode::Char('g'));
        assert_eq!(d.app.overlay, Overlay::ReviewNotes);
        assert!(d.app.review_notes.notice.contains("ambiguous"));
        d.key(KeyCode::Char('a'));
        d.key(KeyCode::Enter); // Default cancel.
        assert_eq!(notes(&d).notes[0].previous.len(), 3);
        d.key(KeyCode::Char('a'));
        d.key(KeyCode::Down);
        d.key(KeyCode::Enter);
        assert_eq!(
            d.app.review_notes.nav.current(),
            Page::Detail,
            "{}",
            d.app.review_notes.notice
        );
        assert!(d.app.deck.progress.completed.contains(&32));
        assert_eq!(notes(&d).notes[0].current.anchor.quote, "the golden key");
        assert_eq!(
            notes(&d).notes[0].previous[0].anchor.quote,
            "The silver key"
        );
        d.key(KeyCode::Esc);
        d.key(KeyCode::Esc);
        d.ctrl('z');
        assert_eq!(d.app.editor.text(), original);
        d.ctrl('s');
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let mut restarted = Desk::open(d.root.clone(), w, h);
        restarted.app.run_action(palette::Action::ReviewNotes);
        restarted.key(KeyCode::Enter);
        assert_eq!(notes(&restarted).notes.len(), 1);
        assert_eq!(notes(&restarted).documents[0].id, 1);
    }
}
#[test]
fn annotation_form_rejects_stale_revision_backing_metadata_and_unsafe_quit() {
    let mut d = writer("review-notes-stale", 80, 24);
    let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    let bytes = fs::read(&path).unwrap();
    d.app.run_action(palette::Action::ReviewNotes);
    d.key(KeyCode::Char('n'));
    d.app.paste("Draft retained");
    d.ctrl('q');
    d.ctrl('k');
    d.ctrl('d');
    assert!(!d.left);
    assert_eq!(d.app.review_notes.nav.current(), Page::Comment);
    d.app.editor.insert('X');
    d.key(KeyCode::Enter);
    assert_eq!(d.app.review_notes.nav.current(), Page::Comment);
    assert!(!d.root.join(model::PATH).exists());
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    d.app.editor.undo();
    d.app.editor.select((0, 13), (0, 27));
    create(&mut d);
    d.key(KeyCode::Char('e'));
    d.app.paste(" More original commentary.");
    let foreign = b"external annotation bytes";
    fs::write(d.root.join(model::PATH), foreign).unwrap();
    d.key(KeyCode::Enter);
    assert_eq!(d.app.review_notes.nav.current(), Page::Comment);
    assert!(d.shows("More original commentary"));
    assert_eq!(fs::read(d.root.join(model::PATH)).unwrap(), foreign);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    // Independent backing-file edit cannot be bypassed by a new form.
    fs::remove_file(d.root.join(model::PATH)).unwrap();
    d.app.review_notes = app::review_notes::Review::load(&d.root);
    fs::write(&path, "External original source").unwrap();
    d.app.run_action(palette::Action::ReviewNotes);
    d.key(KeyCode::Char('n'));
    assert_eq!(d.app.review_notes.nav.current(), Page::Queue);
    assert!(d.app.review_notes.notice.contains("backing source changed"));
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "External original source"
    );
}
#[test]
fn owned_tree_rename_keeps_note_identity_and_unknown_metadata_blocks_move_without_source_loss() {
    let mut d = writer("review-notes-rename", 100, 30);
    create(&mut d);
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    let original = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    d.app.start_rename();
    d.app.on_overlay_key(Key::Char('N'));
    d.app.on_overlay_key(Key::Char('e'));
    d.app.on_overlay_key(Key::Char('w'));
    d.app.on_overlay_key(Key::Enter);
    let after = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    assert_ne!(after, original);
    assert_eq!(d.root.join(&notes(&d).document(1).unwrap().path), after);
    assert_eq!(notes(&d).notes[0].current.document_id, 1);
    d.app.run_action(palette::Action::ReviewNotes);
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('g'));
    assert_eq!(d.app.overlay, Overlay::None);
    fs::write(d.root.join(model::PATH), "unknown original").unwrap();
    assert!(project::rename(&after, "refused").is_err());
    assert!(after.exists());
    assert_eq!(
        fs::read_to_string(d.root.join(model::PATH)).unwrap(),
        "unknown original"
    );
}
#[test]
fn unavailable_notes_do_not_disable_manuscript_save_help_or_secondary_ownership() {
    let (mut d, secondary) = secondary_book("review-notes-secondary", 80, 24, false);
    d.app.run_action(palette::Action::EditSecondary(secondary));
    d.draw();
    d.app.editor.set_text(SOURCE);
    d.app.flush();
    d.app.save();
    d.app.editor.select((0, 13), (0, 27));
    let pane = d.app.views.active;
    let primary = d.app.view_position(grimoire_core::documents::Pane::Primary);
    create(&mut d);
    d.key(KeyCode::F(1));
    assert!(matches!(d.app.overlay, Overlay::Help { .. }));
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::ReviewNotes);
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    assert_eq!(d.app.views.active, pane);
    assert_eq!(
        d.app.view_position(grimoire_core::documents::Pane::Primary),
        primary
    );
    fs::write(d.root.join(model::PATH), "corrupt").unwrap();
    d.app.run_action(palette::Action::ReviewNotes);
    d.key(KeyCode::Char('r'));
    assert!(d.app.review_notes.store.is_none());
    d.key(KeyCode::Esc);
    d.app.editor.clear_selection();
    d.app.paste(" Original retained despite unavailable notes.");
    d.ctrl('s');
    let node = &d.app.project.nodes[d.app.open.unwrap()];
    assert!(
        fs::read_to_string(&node.path)
            .unwrap()
            .contains("Original retained despite unavailable notes.")
    );
}

#[test]
fn review_and_markdown_export_lessons_open_real_workflows_without_awarding_completion() {
    let mut d = writer("review-notes-lessons", 80, 24);
    fs::write(
        d.root.join(".writingdeck-demo"),
        "WritingDeck safe tutorial project v1\n",
    )
    .unwrap();
    for id in [32, 35] {
        d.app.overlay = Overlay::Deck(app::writingdeck::Page::Lesson { id });
        d.key(KeyCode::Enter);
        assert_eq!(d.app.deck.progress.active, Some(id));
        assert!(!d.app.deck.progress.completed.contains(&id));
        if id == 32 {
            assert_eq!(d.app.overlay, Overlay::ReviewNotes);
        } else {
            assert!(matches!(d.app.overlay, Overlay::Export { .. }));
        }
    }
}
