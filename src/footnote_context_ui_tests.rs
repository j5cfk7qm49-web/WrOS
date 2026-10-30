use super::*;

fn writer(tag: &str, w: u16, h: u16) -> Desk {
    let mut d = metric_writer(tag, w, h);
    let body = format!(
        "λ Intro[^A] and again[^a].\n\n[^a]: First public paragraph. %% PRIVATE SECRET %%\n\n    {}END_CONTEXT\n",
        "Original continuation 🦊. ".repeat(80)
    );
    d.app.editor.set_text(&body);
    d.app.save();
    d.app.editor.place(0, 8);
    d.draw();
    d
}
#[test]
fn footnote_context_dock_scroll_modal_navigation_original_edit_and_undo_at_four_sizes() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = writer(&format!("footnote-context-{w}"), w, h);
        let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
        let bytes = fs::read(&path).unwrap();
        let original = d.app.editor.text();
        let caret = (d.app.editor.cy, d.app.editor.cx, d.app.editor.scroll);
        d.app.run_action(palette::Action::FootnoteAtCaret);
        d.draw();
        assert!(d.shows("Footnote context · Source Markdown"));
        assert!(d.shows("λ Intro"), "original writing stays above the dock");
        let Overlay::FootnoteReader(reader) = &d.app.overlay else {
            panic!("reader expected")
        };
        assert!(!reader.definition.contains("PRIVATE SECRET"));
        d.key(KeyCode::End);
        assert!(d.shows("END_CONTEXT"), "{w}x{h}\n{}", d.rows().join("\n"));
        for _ in 0..5 {
            d.key(KeyCode::PageDown);
        }
        assert!(d.shows("END_CONTEXT"));
        d.app.paste("rdoverwrite");
        d.ctrl('x');
        d.ctrl('z');
        d.app.on_drag(10, 10);
        assert_eq!(d.app.editor.text(), original);
        assert_eq!(
            (d.app.editor.cy, d.app.editor.cx, d.app.editor.scroll),
            caret
        );
        d.key(KeyCode::F(1));
        d.ctrl('z');
        d.key(KeyCode::Esc);
        assert!(matches!(d.app.overlay, Overlay::FootnoteReader(_)));
        d.key(KeyCode::Right);
        d.key(KeyCode::Char('r'));
        assert_eq!(d.app.overlay, Overlay::None);
        assert_eq!(d.app.editor.selection(), Some(((0, 21), (0, 25))));
        d.app.run_action(palette::Action::FootnoteAtCaret);
        d.key(KeyCode::Char('d'));
        assert_eq!(d.app.overlay, Overlay::None);
        assert_eq!(d.app.editor.cy, 2);
        d.app.paste("Added λ. ");
        d.ctrl('s');
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("[^a]: Added λ. First public")
        );
        d.app.run_action(palette::Action::FootnoteAtCaret);
        d.draw();
        assert!(d.shows("Added λ."));
        d.key(KeyCode::Esc);
        d.ctrl('z');
        d.ctrl('s');
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let mut restarted = Desk::open(d.root.clone(), w, h);
        restarted.app.focus = Focus::Editor;
        restarted.app.editor.place(0, 8);
        restarted.app.run_action(palette::Action::FootnoteAtCaret);
        restarted.draw();
        assert!(restarted.shows("First public paragraph."));
    }
}
#[test]
fn footnote_context_browser_back_and_stale_source_refusal_keep_the_editor() {
    let mut d = writer("footnote-context-stale", 80, 24);
    d.app.run_action(palette::Action::Footnotes);
    d.key(KeyCode::Down);
    d.key(KeyCode::Char('v'));
    assert!(matches!(d.app.overlay, Overlay::FootnoteReader(_)));
    d.key(KeyCode::Esc);
    assert!(matches!(d.app.overlay, Overlay::Footnotes { sel: 1, .. }));
    d.key(KeyCode::Char('v'));
    let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    let caret = (d.app.editor.cy, d.app.editor.cx);
    let text = d.app.editor.text();
    fs::write(&path, "External original").unwrap();
    d.key(KeyCode::Char('d'));
    assert!(matches!(d.app.overlay, Overlay::FootnoteReader(_)));
    assert!(d.app.msg.contains("backing source changed"));
    assert_eq!((d.app.editor.cy, d.app.editor.cx), caret);
    assert_eq!(d.app.editor.text(), text);
    assert_eq!(fs::read_to_string(path).unwrap(), "External original");
}
#[test]
fn footnote_context_secondary_keeps_primary_and_duplicate_missing_unused_actions_are_honest() {
    let (mut d, secondary) = secondary_book("footnote-context-secondary", 100, 30, false);
    let primary = d.app.editor.text();
    d.app.run_action(palette::Action::EditSecondary(secondary));
    d.draw();
    d.app
        .editor
        .set_text("Text[^a].\n\n[^a]: First.\n[^A]: Duplicate.\n");
    d.app.save();
    d.app.editor.place(0, 5);
    d.app.run_action(palette::Action::FootnoteAtCaret);
    d.key(KeyCode::Char('d'));
    assert!(matches!(d.app.overlay, Overlay::FootnoteReader(_)));
    assert!(d.app.msg.contains("no unique definition"));
    d.key(KeyCode::Esc);
    d.app.editor.set_text("Text[^missing].");
    d.app.save();
    d.app.editor.place(0, 5);
    d.app.run_action(palette::Action::FootnoteAtCaret);
    d.draw();
    assert!(d.shows("No public definition"));
    d.key(KeyCode::Char('d'));
    assert!(matches!(d.app.overlay, Overlay::FootnoteReader(_)));
    d.key(KeyCode::Esc);
    d.app.editor.set_text("[^unused]: Original unused note.");
    d.app.save();
    d.app.editor.place(0, 15);
    d.app.run_action(palette::Action::FootnoteAtCaret);
    d.key(KeyCode::Char('r'));
    assert!(d.app.msg.contains("no reference"));
    d.key(KeyCode::Char('d'));
    assert_eq!(d.app.overlay, Overlay::None);
    assert_eq!(
        d.app
            .view_editor(grimoire_core::documents::Pane::Primary)
            .unwrap()
            .text(),
        primary
    );
}
