use super::*;
use crate::app::{backup::FormKind, shell::Route, writingdeck::Page};
use grimoire_core::documents::Pane;

#[test]
fn google_authorization_is_masked_owned_by_the_form_and_discarded_on_escape_at_all_sizes() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("backup-secret-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.typed("Original backup writing context. ");
        desk.app.editor.select((10, 3), (8, 7));
        desk.draw();
        let text = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary);
        let secondary = desk.app.view_position(Pane::Secondary);
        desk.app.open_settings();
        desk.app.deck.shell.nav.open(Route::Backups);
        desk.app.open_backup_form(FormKind::Google, String::new());
        desk.app
            .paste(r#"{"access_token":"PRIVATE_ACCESS_FIXTURE"}"#);
        desk.draw();
        assert!(desk.shows("[hidden]"));
        assert!(!desk.rows().join("\n").contains("PRIVATE_ACCESS_FIXTURE"));
        desk.app.clip = "CLIPBOARD_SECRET_FIXTURE".into();
        desk.ctrl('v');
        assert!(!desk.rows().join("\n").contains("CLIPBOARD_SECRET_FIXTURE"));
        assert_eq!(desk.app.editor.text(), text);
        desk.key(KeyCode::Esc);
        assert!(desk.app.deck.shell.backup.form.is_none());
        assert_eq!(desk.app.deck.shell.nav.current(), Route::Backups);
        desk.key(KeyCode::Esc);
        desk.key(KeyCode::Esc);
        assert_eq!(desk.app.overlay, Overlay::None);
        assert_eq!(desk.app.view_position(Pane::Primary), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary), secondary);
        desk.ctrl('z');
        assert_ne!(desk.app.editor.text(), text);
    }
}

#[test]
fn cancelled_repository_setup_and_restore_forms_do_not_write_files_or_change_the_book() {
    let mut desk = Desk::open(book("backup-cancel", true), 80, 24);
    let original = desk.app.editor.text();
    desk.app.open_settings();
    desk.app.deck.shell.nav.open(Route::Backups);
    for kind in [
        FormKind::NewLocal,
        FormKind::NewDrive,
        FormKind::Connect,
        FormKind::Restore,
        FormKind::ExportKey,
    ] {
        desk.app.open_backup_form(kind, "a".repeat(64));
        desk.app.paste("/tmp/must-not-be-created");
        desk.key(KeyCode::Tab);
        desk.key(KeyCode::Esc);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::Backups);
        assert!(desk.app.deck.shell.backup.form.is_none());
        assert!(desk.app.deck.shell.backup.job.is_none());
        assert_eq!(desk.app.editor.text(), original);
    }
}

#[test]
fn backup_refuses_source_conflicts_and_preserves_external_and_unsaved_words() {
    let mut desk = Desk::open(book("backup-conflict", true), 100, 30);
    desk.app.focus = Focus::Editor;
    desk.typed("Unsaved local work retained. ");
    let path = desk.app.project.nodes[desk.app.open.unwrap()].path.clone();
    fs::write(&path, "External writing must remain.\n").unwrap();
    desk.app.overlay = Overlay::Deck(Page::Settings);
    desk.app.deck.shell.nav.reset(Route::Backups);
    desk.app.request_project_backup();
    assert!(desk.app.deck.shell.backup.job.is_none());
    assert!(desk.app.deck.shell.backup.notice.contains("conflicts"));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "External writing must remain.\n"
    );
    assert!(
        fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| fs::read_to_string(entry.path())
                .is_ok_and(|text| text.contains("Unsaved local work retained.")))
    );
}
