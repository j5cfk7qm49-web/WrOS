use super::*;
use crate::app::{
    shell::{Category, Route},
    writingdeck::Page,
};
use grimoire_core::documents::Pane;

fn system_panel(desk: &mut Desk, panel: usize) {
    desk.app.open_settings();
    desk.draw();
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Enter);
    for _ in 0..panel {
        desk.key(KeyCode::Down);
    }
    desk.key(KeyCode::Enter);
}

fn close_settings(desk: &mut Desk) {
    for _ in 0..3 {
        desk.key(KeyCode::Esc);
    }
    assert_eq!(desk.app.overlay, Overlay::None);
}

#[test]
fn unified_settings_preserve_both_editors_positions_focus_selection_scroll_and_undo() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("shell-retained-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.typed("New original λ words. ");
        desk.app
            .with_editor_view(Pane::Primary, |app| app.editor.select((12, 8), (9, 4)));
        desk.app.editor.select((10, 3), (8, 7));
        desk.app.scroll_editor_view(Pane::Primary, true, 45);
        desk.app.scroll_editor_view(Pane::Secondary, true, 38);
        desk.draw();
        let primary = desk.app.view_position(Pane::Primary).unwrap();
        let secondary = desk.app.view_position(Pane::Secondary).unwrap();
        let source = desk.app.editor.text();
        let workspace = desk.app.deck.state.selected;
        system_panel(&mut desk, 0);
        desk.typed("PASSWORD-LIKE-TEXT-MUST-NOT-ENTER-A-HIDDEN-EDITOR");
        desk.ctrl('x');
        assert_eq!(
            desk.app.editor.text(),
            source,
            "cut is owned by the modal, never the hidden editor"
        );
        assert_eq!(desk.app.view_position(Pane::Secondary).unwrap(), secondary);
        desk.ctrl('z');
        assert_eq!(desk.app.editor.text(), source);
        close_settings(&mut desk);
        assert_eq!(desk.app.view_position(Pane::Primary).unwrap(), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary).unwrap(), secondary);
        assert_eq!(desk.app.deck.state.selected, workspace);
        assert_eq!(desk.app.focus, Focus::Secondary);
        desk.ctrl('z');
        assert_ne!(
            desk.app.editor.text(),
            source,
            "original edit still has its undo history"
        );
    }
}

#[test]
fn unified_settings_reuse_existing_preferences_and_modal_cancel_paths() {
    let mut desk = Desk::open(book("shell-existing-settings", true), 100, 30);
    desk.app.open_settings();
    desk.key(KeyCode::Enter);
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Down);
    let old_width = desk.app.line_width;
    desk.key(KeyCode::Enter);
    assert_ne!(desk.app.line_width, old_width);
    assert_eq!(desk.app.overlay, Overlay::Deck(Page::Settings));
    assert_eq!(
        desk.app.deck.shell.nav.current(),
        Route::Category(Category::Writing)
    );
    assert_eq!(desk.app.deck.shell.nav.selected(), 2);
    desk.key(KeyCode::Esc);
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Enter);
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Enter);
    assert!(matches!(desk.app.overlay, Overlay::Themes { .. }));
    desk.key(KeyCode::Esc);
    assert_eq!(desk.app.overlay, Overlay::Deck(Page::Settings));
    assert_eq!(
        desk.app.deck.shell.nav.current(),
        Route::Category(Category::Interface)
    );
    assert_eq!(desk.app.deck.shell.nav.selected(), 2);
}

#[test]
fn unified_settings_preserve_future_and_externally_changed_preferences() {
    let root = book("shell-future", true);
    let path = root.join(".grimoire/writingdeck/shell.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let future = "{\"version\":99,\"selections\":[],\"future\":\"kept\"}\n";
    fs::write(&path, future).unwrap();
    let mut desk = Desk::open(root, 80, 24);
    desk.app.open_settings();
    desk.key(KeyCode::Esc);
    assert_eq!(fs::read_to_string(&path).unwrap(), future);
    let root = book("shell-external", true);
    let path = root.join(".grimoire/writingdeck/shell.json");
    let mut desk = Desk::open(root, 80, 24);
    desk.app.open_settings();
    desk.key(KeyCode::Esc);
    let external = "{\"version\":1,\"selections\":[]}\n";
    fs::write(&path, external).unwrap();
    desk.app.open_settings();
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Esc);
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
}

#[test]
fn unified_settings_backend_failure_is_native_and_writing_remains_available() {
    struct Failed;
    impl writingdeck_sys::Backend for Failed {
        fn request(
            &mut self,
            _: writingdeck_sys::Request,
        ) -> anyhow::Result<writingdeck_sys::Reply> {
            anyhow::bail!("fixture service disconnected")
        }
    }
    let mut desk = Desk::open(book("shell-failed-service", true), 80, 24);
    desk.app.focus = Focus::Editor;
    desk.app.deck.use_test_backend(Box::new(Failed));
    system_panel(&mut desk, 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while desk.app.deck.busy && std::time::Instant::now() < deadline {
        desk.app.tick_deck();
        std::thread::yield_now();
    }
    desk.draw();
    assert!(desk.shows("fixture service disconnected"));
    assert!(desk.app.deck.snapshot.is_none());
    close_settings(&mut desk);
    desk.typed("Writing remains available offline. ");
    assert!(
        desk.app
            .editor
            .text()
            .contains("Writing remains available offline.")
    );
}

#[test]
fn unified_settings_power_confirmation_and_display_unavailability_are_honest() {
    let mut desk = Desk::open(book("shell-power-display", true), 80, 24);
    system_panel(&mut desk, 2);
    for _ in 0..4 {
        desk.key(KeyCode::Down);
    }
    desk.key(KeyCode::Enter);
    assert!(matches!(
        desk.app.overlay,
        Overlay::Deck(Page::Power { yes: false, .. })
    ));
    desk.key(KeyCode::Esc);
    assert_eq!(desk.app.overlay, Overlay::Deck(Page::Settings));
    desk.key(KeyCode::Esc);
    desk.key(KeyCode::Up);
    desk.key(KeyCode::Enter);
    desk.key(KeyCode::End);
    desk.key(KeyCode::Enter);
    assert!(desk.shows("display service pending"));
    assert!(!desk.app.deck.busy, "no fabricated warmth request was sent");
}
