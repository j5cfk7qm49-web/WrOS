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
    assert!(desk.shows("Warm screen"));
    assert!(!desk.app.deck.busy, "no fabricated warmth request was sent");
}

fn wait_system(desk: &mut Desk) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while desk.app.deck.busy && std::time::Instant::now() < end {
        desk.app.tick_deck();
        std::thread::yield_now();
    }
    assert!(!desk.app.deck.busy);
    desk.draw();
}
fn open_warm(desk: &mut Desk) {
    system_panel(desk, 1);
    wait_system(desk);
    desk.key(KeyCode::End);
    desk.key(KeyCode::Enter);
    wait_system(desk);
    assert_eq!(desk.app.deck.shell.nav.current(), Route::WarmScreen);
}
fn choose_warm_row(desk: &mut Desk, row: usize) {
    desk.key(KeyCode::Home);
    for _ in 0..row {
        desk.key(KeyCode::Down);
    }
    desk.key(KeyCode::Enter);
}

#[test]
fn warm_screen_preview_confirm_cancel_and_timeout_retain_both_editors() {
    use std::time::{Duration, Instant};
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) = secondary_book(&format!("warm-retain-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.typed("Warm light; original words. ");
        desk.app.editor.select((10, 3), (8, 7));
        desk.draw();
        let before = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary).unwrap();
        let secondary = desk.app.view_position(Pane::Secondary).unwrap();
        desk.app.deck.simulate();
        open_warm(&mut desk);
        assert_eq!(desk.app.deck.shell.warmth.kelvin, 2200);
        choose_warm_row(&mut desk, 4);
        assert_eq!(desk.app.deck.shell.warmth.kelvin, 2100);
        choose_warm_row(&mut desk, 8);
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WarmPreview);
        desk.ctrl('x');
        assert_eq!(desk.app.editor.text(), before);
        desk.typed("should not enter manuscript");
        assert_eq!(desk.app.editor.text(), before);
        desk.key(KeyCode::Esc);
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.warmth.kelvin, 2200);
        choose_warm_row(&mut desk, 4);
        choose_warm_row(&mut desk, 8);
        wait_system(&mut desk);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.warmth.kelvin, 2100);
        choose_warm_row(&mut desk, 8);
        wait_system(&mut desk);
        desk.app.deck.shell.warm_preview.as_mut().unwrap().1 =
            Instant::now() - Duration::from_secs(1);
        desk.app.tick_deck();
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WarmScreen);
        for _ in 0..4 {
            desk.key(KeyCode::Esc);
        }
        assert_eq!(desk.app.overlay, Overlay::None);
        assert_eq!(desk.app.editor.text(), before);
        assert_eq!(desk.app.view_position(Pane::Primary).unwrap(), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary).unwrap(), secondary);
        assert_eq!(desk.app.focus, Focus::Secondary);
        desk.ctrl('z');
        assert_ne!(desk.app.editor.text(), before);
    }
}

#[test]
fn warm_screen_missing_service_has_no_preview_and_leaves_writing_available() {
    let mut desk = Desk::open(book("warm-unavailable", true), 80, 24);
    open_warm(&mut desk);
    choose_warm_row(&mut desk, 8);
    assert!(!desk.app.deck.busy);
    assert!(desk.app.deck.shell.warm_preview.is_none());
    assert!(desk.shows("unavailable or unchecked"));
}

#[test]
fn late_warm_preview_is_cancelled_without_reopening_settings() {
    use std::sync::{Arc, Mutex, mpsc};
    use writingdeck_sys::{Backend, Reply, Request, Simulated};
    struct Delayed {
        backend: Simulated,
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        calls: Arc<Mutex<Vec<Request>>>,
    }
    impl Backend for Delayed {
        fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
            self.calls.lock().unwrap().push(request.clone());
            if matches!(request, Request::DisplayPreview(_)) {
                self.entered.send(()).unwrap();
                self.release
                    .recv_timeout(std::time::Duration::from_secs(3))
                    .unwrap();
            }
            self.backend.request(request)
        }
    }
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut desk = Desk::open(book("warm-late", true), 80, 24);
    desk.app.deck.use_test_backend(Box::new(Delayed {
        backend: Simulated::laptop(),
        entered: entered_tx,
        release: release_rx,
        calls: calls.clone(),
    }));
    open_warm(&mut desk);
    choose_warm_row(&mut desk, 8);
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    desk.key(KeyCode::Esc);
    desk.key(KeyCode::Esc);
    desk.key(KeyCode::Esc);
    desk.key(KeyCode::Esc);
    assert_eq!(desk.app.overlay, Overlay::None);
    release_tx.send(()).unwrap();
    wait_system(&mut desk);
    assert_eq!(desk.app.overlay, Overlay::None);
    assert!(desk.app.deck.shell.warm_preview.is_none());
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .any(|r| matches!(r, Request::DisplayCancel(_)))
    );
}

#[test]
fn unavailable_suspend_never_reaches_the_worker_and_retains_editors_at_four_sizes() {
    use std::sync::{Arc, Mutex};
    use writingdeck_sys::{Backend, PowerAction, Reply, Request, Simulated};
    struct Recorded(Arc<Mutex<Vec<Request>>>, Simulated);
    impl Backend for Recorded {
        fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
            self.0.lock().unwrap().push(request.clone());
            self.1.request(request)
        }
    }
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("suspend-guard-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.typed("Keep both original editors. ");
        desk.app.editor.select((10, 3), (8, 7));
        desk.draw();
        let original = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary).unwrap();
        let secondary = desk.app.view_position(Pane::Secondary).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        desk.app
            .deck
            .use_test_backend(Box::new(Recorded(calls.clone(), Simulated::laptop())));
        system_panel(&mut desk, 2);
        wait_system(&mut desk);
        choose_warm_row(&mut desk, 3); // Native Power: unavailable Suspend row.
        assert_eq!(desk.app.overlay, Overlay::Deck(Page::Settings));
        assert!(desk.shows("secure screen locking"));
        close_settings(&mut desk);
        desk.app.overlay = Overlay::Deck(Page::System { sel: 8 });
        desk.key(KeyCode::Enter);
        assert!(matches!(
            desk.app.overlay,
            Overlay::Deck(Page::System { .. })
        ));
        assert!(desk.shows("secure screen locking"));
        // A future caller cannot bypass the menu by creating an old confirmed
        // modal. The Linux adapter separately refuses direct typed requests.
        desk.app.overlay = Overlay::Deck(Page::Power {
            action: PowerAction::Suspend,
            yes: true,
        });
        desk.draw();
        assert!(desk.shows("Suspend unavailable"));
        assert!(!desk.shows("YES, proceed"));
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert!(
            !calls
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, Request::Power(_)))
        );
        desk.key(KeyCode::Esc);
        desk.key(KeyCode::Esc);
        assert_eq!(desk.app.overlay, Overlay::None);
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary).unwrap(), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary).unwrap(), secondary);
        assert_eq!(desk.app.focus, Focus::Secondary);
        desk.ctrl('z');
        assert_ne!(desk.app.editor.text(), original);
    }
}

#[test]
fn supported_power_actions_still_require_confirmation_and_successful_saves() {
    use std::sync::{Arc, Mutex};
    use writingdeck_sys::{Backend, PowerAction, Reply, Request, Simulated};
    struct Recorded(Arc<Mutex<Vec<Request>>>, Simulated);
    impl Backend for Recorded {
        fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
            self.0.lock().unwrap().push(request.clone());
            self.1.request(request)
        }
    }
    for (selection, expected) in [(9, PowerAction::Shutdown), (10, PowerAction::Restart)] {
        let mut desk = Desk::open(book(&format!("power-guard-{selection}"), true), 100, 30);
        let calls = Arc::new(Mutex::new(Vec::new()));
        desk.app
            .deck
            .use_test_backend(Box::new(Recorded(calls.clone(), Simulated::laptop())));
        desk.app.open_system_settings();
        wait_system(&mut desk);
        desk.app.overlay = Overlay::Deck(Page::System { sel: selection });
        desk.key(KeyCode::Enter);
        desk.key(KeyCode::Enter); // Default is Cancel.
        assert!(
            !calls
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, Request::Power(_)))
        );
        desk.app.overlay = Overlay::None;
        desk.app.focus = Focus::Editor;
        desk.typed("New original before power confirmation. ");
        desk.app.overlay = Overlay::Deck(Page::System { sel: selection });
        desk.key(KeyCode::Enter);
        desk.key(KeyCode::Right);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert!(calls.lock().unwrap().contains(&Request::Power(expected)));
        assert!(
            fs::read_to_string(first_scene(&desk.root))
                .unwrap()
                .contains("New original before power confirmation.")
        );
        calls.lock().unwrap().clear();
        // External source replacement triggers the existing save conflict gate.
        desk.app.overlay = Overlay::None;
        desk.app.focus = Focus::Editor;
        desk.typed("Unsaved source with an external conflict. ");
        fs::write(
            first_scene(&desk.root),
            "External source must be preserved.\n",
        )
        .unwrap();
        desk.app.overlay = Overlay::Deck(Page::Power {
            action: PowerAction::Restart,
            yes: true,
        });
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert!(
            !calls
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, Request::Power(_)))
        );
        assert_eq!(
            fs::read_to_string(first_scene(&desk.root)).unwrap(),
            "External source must be preserved.\n"
        );
        assert!(desk.shows("Power unchanged"));
        assert!(
            fs::read_dir(first_scene(&desk.root).parent().unwrap())
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| fs::read_to_string(entry.path())
                    .is_ok_and(|text| text.contains("Unsaved source with an external conflict.")))
        );
    }
}

#[test]
fn power_waits_for_missing_sources_and_failed_document_claims_then_allows_explicit_retry() {
    use std::sync::{Arc, Mutex};
    use writingdeck_sys::{Backend, PowerAction, Reply, Request, Simulated};
    struct Recorded(Arc<Mutex<Vec<Request>>>, Simulated);
    impl Backend for Recorded {
        fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
            self.0.lock().unwrap().push(request.clone());
            self.1.request(request)
        }
    }
    for directory in [false, true] {
        let mut desk = Desk::open(book(&format!("power-backing-{directory}"), true), 100, 30);
        let calls = Arc::new(Mutex::new(Vec::new()));
        desk.app
            .deck
            .use_test_backend(Box::new(Recorded(calls.clone(), Simulated::laptop())));
        desk.app.open_system_settings();
        wait_system(&mut desk);
        desk.app.overlay = Overlay::None;
        desk.app.focus = Focus::Editor;
        desk.typed("Retain this original while storage is unavailable. ");
        let original = desk.app.editor.text();
        let position = desk.app.view_position(Pane::Primary).unwrap();
        let path = first_scene(&desk.root);
        let held = path.with_extension("fixture-held");
        fs::rename(&path, &held).unwrap();
        let stored = fs::read(&held).unwrap();
        if directory {
            fs::create_dir(&path).unwrap();
        }
        desk.app.overlay = Overlay::Deck(Page::Power {
            action: PowerAction::Shutdown,
            yes: true,
        });
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert!(
            !calls
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, Request::Power(_)))
        );
        assert!(desk.shows("Power unchanged"));
        assert!(desk.app.project.dirty_count() > 0);
        assert!(matches!(
            desk.app.save_state,
            app::SaveState::Failed(_) | app::SaveState::Waiting(_)
        ));
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary).unwrap(), position);
        assert_eq!(fs::read(&held).unwrap(), stored);
        if directory {
            fs::remove_dir(&path).unwrap();
        }
        assert!(!path.exists(), "no unchecked backing source was recreated");
        fs::rename(&held, &path).unwrap();
        desk.key(KeyCode::Enter); // Explicit retry through the retained confirmation.
        wait_system(&mut desk);
        assert!(
            calls
                .lock()
                .unwrap()
                .contains(&Request::Power(PowerAction::Shutdown))
        );
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("Retain this original while storage is unavailable.")
        );
        desk.key(KeyCode::Esc);
        desk.key(KeyCode::Esc);
        desk.ctrl('z');
        assert_ne!(
            desk.app.editor.text(),
            original,
            "the retained original still has undo"
        );
    }
}
