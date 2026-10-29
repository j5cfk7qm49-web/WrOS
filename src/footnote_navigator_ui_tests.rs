use super::*;

const BODY: &str = "λ[^long  label] original prose.\n\n[^long label]: Original definition.\n";
fn writer(tag: &str, w: u16, h: u16) -> Desk {
    let mut d = metric_writer(tag, w, h);
    d.app.editor.set_text(BODY);
    d.app.save();
    d.app.editor.place(0, 1);
    d
}

#[test]
fn footnote_navigation_guards_modal_input_and_edits_the_original_definition_at_four_sizes() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = writer(&format!("footnote-navigator-{w}"), w, h);
        let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
        let before = fs::read(&path).unwrap();
        d.app.editor.select((0, 1), (0, 4));
        let selected = d.app.editor.selection();
        d.app.run_action(palette::Action::Footnotes);
        d.draw();
        assert!(d.shows("Footnotes · Original Markdown"));
        d.app.paste("dnew");
        d.ctrl('x');
        d.ctrl('z');
        d.app.on_drag(10, 10);
        assert_eq!(d.app.editor.text(), BODY);
        assert_eq!(d.app.editor.selection(), selected);
        d.key(KeyCode::F(1));
        d.ctrl('z');
        d.ctrl('x');
        d.app.paste("hidden commands");
        d.key(KeyCode::Esc);
        assert!(matches!(d.app.overlay, Overlay::Footnotes { .. }));
        assert_eq!(d.app.editor.text(), BODY);
        d.key(KeyCode::Enter);
        assert_eq!(
            d.app.editor.selection(),
            Some(((0, 1), (0, "λ[^long  label]".chars().count())))
        );
        d.app.run_action(palette::Action::Footnotes);
        d.key(KeyCode::Char('d'));
        assert_eq!(d.app.overlay, Overlay::None);
        assert_eq!(d.app.editor.cy, 2);
        d.app.paste("Added λ. ");
        d.ctrl('s');
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("[^long label]: Added λ. Original definition.")
        );
        d.ctrl('z');
        d.ctrl('s');
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn footnote_jump_refuses_stale_buffer_disk_missing_and_ambiguous_definitions() {
    let mut d = writer("footnote-navigator-stale", 80, 24);
    let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    d.app.run_action(palette::Action::Footnotes);
    d.app.editor.insert('X');
    let position = (d.app.editor.cy, d.app.editor.cx);
    d.key(KeyCode::Enter);
    assert!(matches!(d.app.overlay, Overlay::Footnotes { .. }));
    assert!(d.app.msg.contains("source revision changed"));
    assert_eq!((d.app.editor.cy, d.app.editor.cx), position);
    d.key(KeyCode::Esc);
    d.app.editor.undo();
    d.app.run_action(palette::Action::Footnotes);
    fs::write(&path, "Other original source").unwrap();
    d.key(KeyCode::Char('d'));
    assert!(matches!(d.app.overlay, Overlay::Footnotes { .. }));
    assert!(d.app.msg.contains("backing source changed"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "Other original source");
    fs::remove_file(&path).unwrap();
    d.key(KeyCode::Enter);
    assert!(matches!(d.app.overlay, Overlay::Footnotes { .. }));
    assert_eq!(d.app.editor.text(), BODY);

    let mut d = writer("footnote-navigator-duplicate", 80, 24);
    d.app
        .editor
        .set_text("Text[^one].\n\n[^one]: First.\n[^ONE]: Second.\n");
    d.app.save();
    d.app.run_action(palette::Action::Footnotes);
    d.key(KeyCode::Char('d'));
    assert!(matches!(d.app.overlay, Overlay::Footnotes { .. }));
    assert!(d.app.msg.contains("no unique definition"));
}
