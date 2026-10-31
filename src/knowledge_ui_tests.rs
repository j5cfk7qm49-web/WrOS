use super::*;
use app::continuity::Page;
use grimoire_core::knowledge::{Knowledge, Moment, PATH, State};

const SOURCE: &str = "λ Original sentence. %% private original %%\n";
fn writer(tag: &str, width: u16, height: u16) -> Desk {
    let mut d = metric_writer(tag, width, height);
    d.app.editor.set_text(SOURCE);
    d.app.save();
    d
}
fn fact(d: &mut Desk) {
    d.app.run_action(palette::Action::Continuity);
    d.key(KeyCode::Char('n'));
    d.key(KeyCode::Left);
    d.key(KeyCode::Left);
    for value in ["Key", "location", "Archive", "canon", "FACT_META_ONLY"] {
        d.app.paste(value);
        d.key(KeyCode::Tab);
    }
    d.key(KeyCode::Enter);
    assert_eq!(
        d.app.continuity.nav.current(),
        Page::Facts,
        "{}",
        d.app.continuity.notice
    );
}
fn knowledge(d: &mut Desk) {
    d.key(KeyCode::Char('g'));
    assert_eq!(
        d.app.continuity.nav.current(),
        Page::EditKnowledge,
        "{}",
        d.app.continuity.notice
    );
    d.app.paste("Wren λ");
    for _ in 0..3 {
        d.key(KeyCode::Tab);
    }
    d.app.paste("KNOWLEDGE_META_ONLY");
    d.key(KeyCode::Enter);
    assert_eq!(
        d.app.continuity.nav.current(),
        Page::Knowledge,
        "{}",
        d.app.continuity.notice
    );
}
#[test]
fn knowledge_create_edit_retained_versions_archive_help_and_source_preserve_writing_at_four_sizes()
{
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = writer(&format!("knowledge-{width}"), width, height);
        d.app.editor.select((0, 0), (0, 1));
        let selection = d.app.editor.selection();
        let caret = (d.app.editor.cy, d.app.editor.cx, d.app.editor.scroll);
        let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
        let before = fs::read(&path).unwrap();
        fact(&mut d);
        knowledge(&mut d);
        let saved = Knowledge::load(&d.root).unwrap();
        assert_eq!(saved.value.entries.len(), 1);
        assert_eq!(saved.value.entries[0].current.character, "Wren λ");
        assert_eq!(saved.value.entries[0].current.state, State::Knows);
        assert_eq!(saved.value.entries[0].current.moment, Moment::After);
        d.key(KeyCode::Char('e'));
        d.key(KeyCode::Tab);
        d.key(KeyCode::Right);
        d.key(KeyCode::Right);
        d.key(KeyCode::Enter);
        let saved = Knowledge::load(&d.root).unwrap();
        assert_eq!(saved.value.entries[0].current.state, State::Unaware);
        assert_eq!(saved.value.entries[0].previous.len(), 1);
        d.key(KeyCode::Char('v'));
        assert_eq!(d.app.continuity.nav.current(), Page::KnowledgeDetail);
        d.key(KeyCode::End);
        assert!(d.shows("EARLIER VERSION 1"));
        assert!(d.shows("Knows the stated fact"));
        assert!(d.shows("Single-value scope: canon"));
        assert!(d.shows("FACT_META_ONLY"));
        d.app.paste("blocked hidden source");
        d.ctrl('x');
        d.ctrl('z');
        d.key(KeyCode::F(1));
        assert!(d.shows("Continuity & draft milestones"));
        d.key(KeyCode::Esc);
        d.key(KeyCode::Esc);
        d.key(KeyCode::Char('d'));
        assert!(
            Knowledge::load(&d.root).unwrap().value.entries[0]
                .current
                .archived
        );
        d.key(KeyCode::Char('h'));
        assert!(d.shows("Archived"));
        assert!(d.shows("+ archived"));
        d.key(KeyCode::Char('d'));
        assert!(
            !Knowledge::load(&d.root).unwrap().value.entries[0]
                .current
                .archived
        );
        assert_eq!(d.app.editor.text(), SOURCE);
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(d.app.editor.selection(), selection);
        assert_eq!(
            (d.app.editor.cy, d.app.editor.cx, d.app.editor.scroll),
            caret
        );
        d.key(KeyCode::Char('g'));
        assert_eq!(d.app.overlay, Overlay::None);
        assert_eq!(d.app.project.nodes[d.app.open.unwrap()].path, path);
        let result = grimoire_core::export::export(
            &d.app.project,
            &grimoire_core::export::ExportOptions {
                docx: false,
                epub: false,
                markdown: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            !fs::read_to_string(&result.files[0])
                .unwrap()
                .contains("META_ONLY")
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
#[test]
fn knowledge_source_revision_fact_and_external_metadata_guards_keep_every_original() {
    for fault in ["revision", "backing", "fact", "metadata"] {
        let mut d = writer(&format!("knowledge-refuse-{fault}"), 100, 30);
        fact(&mut d);
        d.key(KeyCode::Char('g'));
        d.app.paste("Wren");
        let path = d.app.project.nodes[d.app.open.unwrap()].path.clone();
        let metadata = d.root.join(PATH);
        match fault {
            "revision" => d.app.editor.set_text("Author changed source.\n"),
            "backing" => fs::write(&path, "External original.\n").unwrap(),
            "fact" => d
                .app
                .continuity
                .store
                .as_mut()
                .unwrap()
                .update(|s| {
                    s.facts[0].value = "Tower".into();
                    Ok(())
                })
                .unwrap(),
            "metadata" => fs::write(&metadata, b"unknown original knowledge").unwrap(),
            _ => unreachable!(),
        }
        let text = d.app.editor.text();
        let disk = fs::read(&path).unwrap();
        let sidecar = fs::read(&metadata).ok();
        d.key(KeyCode::Enter);
        assert_eq!(
            d.app.continuity.nav.current(),
            Page::EditKnowledge,
            "{fault}"
        );
        assert_eq!(d.app.editor.text(), text, "{fault}");
        assert_eq!(fs::read(&path).unwrap(), disk, "{fault}");
        assert_eq!(fs::read(&metadata).ok(), sidecar, "{fault}");
    }
}
#[test]
fn shared_document_registry_keeps_knowledge_source_after_an_owned_rename_and_reopen() {
    let mut d = writer("knowledge-rename", 100, 30);
    fact(&mut d);
    knowledge(&mut d);
    d.key(KeyCode::Esc);
    let saved = Knowledge::load(&d.root).unwrap();
    let id = saved.value.entries[0].current.document_id;
    let original = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    d.app.start_rename();
    for c in "New".chars() {
        d.app.on_overlay_key(Key::Char(c));
    }
    d.app.on_overlay_key(Key::Enter);
    let after = d.app.project.nodes[d.app.open.unwrap()].path.clone();
    assert_ne!(original, after);
    let mut opened = Desk::open(d.root.clone(), 100, 30);
    opened.app.run_action(palette::Action::Open(after.clone()));
    opened.app.run_action(palette::Action::Continuity);
    opened.key(KeyCode::Char('5'));
    opened.key(KeyCode::Char('g'));
    assert_eq!(
        opened.app.overlay,
        Overlay::None,
        "{}",
        opened.app.continuity.notice
    );
    assert_eq!(
        opened.app.project.nodes[opened.app.open.unwrap()].path,
        after
    );
    assert_eq!(
        Knowledge::load(&d.root).unwrap().value.entries[0]
            .current
            .document_id,
        id
    );
    assert!(
        fs::read_to_string(after)
            .unwrap()
            .contains("private original")
    );
}
#[test]
fn future_knowledge_preserves_ordinary_editor_and_existing_fact_view() {
    let mut d = writer("knowledge-future", 100, 30);
    fact(&mut d);
    d.key(KeyCode::Esc);
    let root = d.root.clone();
    let metadata = root.join(PATH);
    fs::create_dir_all(metadata.parent().unwrap()).unwrap();
    let bytes = b"{\"schema_version\":2,\"next_id\":0,\"entries\":[]}";
    fs::write(&metadata, bytes).unwrap();
    let mut opened = Desk::open(root, 100, 30);
    let path = first_scene(&opened.root);
    opened.app.run_action(palette::Action::Open(path.clone()));
    opened.app.run_action(palette::Action::Continuity);
    opened.draw();
    assert!(opened.shows("Key"));
    opened.key(KeyCode::Char('5'));
    assert!(opened.app.continuity.notice.contains("preserved"));
    opened.key(KeyCode::Esc);
    opened.app.editor.set_text("Author's next original line.\n");
    opened.app.save();
    assert_eq!(fs::read(&metadata).unwrap(), bytes);
    assert!(
        fs::read_to_string(path)
            .unwrap()
            .contains("Author's next original")
    );
}

#[test]
fn knowledge_character_filter_is_a_view_and_accepts_numeric_names_without_changing_source() {
    let mut d = writer("knowledge-filter", 100, 30);
    fact(&mut d);
    knowledge(&mut d);
    let before = d.app.editor.text();
    let bytes = fs::read(d.root.join(PATH)).unwrap();
    d.key(KeyCode::Char('/'));
    d.app.paste("Wren5");
    assert_eq!(d.app.continuity.nav.current(), Page::FilterKnowledge);
    d.key(KeyCode::Enter);
    assert!(d.shows("No matching knowledge"));
    assert!(d.shows("Filtered"));
    d.key(KeyCode::Char('/'));
    for _ in 0..5 {
        d.key(KeyCode::Backspace);
    }
    d.app.paste("wren");
    d.key(KeyCode::Esc);
    assert!(d.shows("Wren λ"));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.continuity.nav.current(), Page::EditKnowledge);
    d.key(KeyCode::Esc);
    assert_eq!(d.app.editor.text(), before);
    assert_eq!(fs::read(d.root.join(PATH)).unwrap(), bytes);
}

#[test]
fn excessive_knowledge_history_refuses_before_a_partial_view_and_preserves_metadata() {
    use grimoire_core::knowledge::Version;
    let mut d = writer("knowledge-history-bound", 100, 30);
    fact(&mut d);
    d.key(KeyCode::Esc);
    let mut fact = d.app.continuity.store.as_ref().unwrap().value.facts[0].clone();
    let document = fact.scene_id.unwrap();
    fact.value = "x".repeat(10000);
    let mut v = Version {
        character: "Wren".into(),
        document_id: document,
        scene_name: "Original scene".into(),
        source_sha256: grimoire_core::knowledge::source_hash(SOURCE),
        fact,
        state: State::Knows,
        moment: Moment::After,
        notes: "Original".into(),
        archived: false,
        recorded_at: "2026-10-30T23:00:00Z".into(),
    };
    let mut store = Knowledge::load(&d.root).unwrap();
    let id = store.update(|s| s.add(v.clone())).unwrap();
    for index in 0..30 {
        v.notes = format!("Original note {index}");
        store.update(|s| s.revise(id, v.clone())).unwrap();
    }
    let bytes = fs::read(d.root.join(PATH)).unwrap();
    let mut opened = Desk::open(d.root.clone(), 100, 30);
    opened.app.run_action(palette::Action::Open(
        d.app.project.nodes[d.app.open.unwrap()].path.clone(),
    ));
    opened.app.run_action(palette::Action::Continuity);
    opened.key(KeyCode::Char('5'));
    opened.key(KeyCode::Char('v'));
    assert_eq!(opened.app.continuity.nav.current(), Page::Knowledge);
    assert!(opened.app.continuity.notice.contains("no partial view"));
    assert_eq!(fs::read(d.root.join(PATH)).unwrap(), bytes);
    assert_eq!(opened.app.editor.text(), SOURCE);
}
