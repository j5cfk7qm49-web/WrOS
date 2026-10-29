use super::*;

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
