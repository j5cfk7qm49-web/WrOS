use super::*;
use crate::app::{
    shell::{Category, Route},
    writingdeck::Page,
};
use grimoire_core::documents::Pane;

fn system_panel(desk: &mut Desk, panel: usize) {
    desk.app.open_settings();
    desk.draw();
    desk.key(KeyCode::Home);
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Enter);
    desk.key(KeyCode::Home);
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

#[test]
fn native_read_only_system_views_keep_editor_state_and_failed_refresh_drops_stale_results() {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use writingdeck_sys::{Backend, Reply, Request, Simulated};
    struct Services {
        calls: Arc<Mutex<Vec<Request>>>,
        fail: Arc<AtomicBool>,
        simulator: Simulated,
    }
    impl Backend for Services {
        fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
            self.calls.lock().unwrap().push(request.clone());
            if request == Request::ScanWifi && self.fail.load(Ordering::Relaxed) {
                anyhow::bail!("scan service stopped");
            }
            self.simulator.request(request)
        }
    }
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut desk = Desk::open(
            book(&format!("native-service-views-{width}"), true),
            width,
            height,
        );
        let calls = Arc::new(Mutex::new(Vec::new()));
        let fail = Arc::new(AtomicBool::new(false));
        desk.app.deck.use_test_backend(Box::new(Services {
            calls: calls.clone(),
            fail: fail.clone(),
            simulator: Simulated::laptop(),
        }));
        desk.app.focus = Focus::Editor;
        desk.typed("Original λ draft. ");
        let original = desk.app.editor.text();
        let position = desk.app.view_position(Pane::Primary);
        system_panel(&mut desk, 0);
        wait_system(&mut desk);
        desk.key(KeyCode::Home);
        desk.choose("Scan Wi-Fi networks");
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiNetworks);
        assert!(desk.shows("WrOS-Studio"));
        assert!(desk.shows("SIMULATED"));
        desk.typed("DO-NOT-ENTER-THE-EDITOR");
        desk.ctrl('z');
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary), position);
        // Selecting a scanned network opens the protected enrollment screen
        // without transmitting credentials or connecting until confirmed.
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiConnect);
        assert!(
            !calls
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, Request::ConnectWifi { .. }))
        );
        desk.key(KeyCode::Esc);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiNetworks);
        fail.store(true, Ordering::Relaxed);
        desk.key(KeyCode::Home);
        desk.key(KeyCode::Enter);
        assert!(desk.app.deck.shell.wifi_networks.is_none());
        wait_system(&mut desk);
        assert!(desk.shows("scan service stopped"));
        assert!(!desk.shows("WrOS-Studio"));
        desk.key(KeyCode::Esc);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::SavedWifi);
        assert!(desk.shows("HomeOffice"));
        for _ in 0..4 {
            desk.key(KeyCode::Esc);
        }
        assert_eq!(desk.app.overlay, Overlay::None);
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary), position);
        desk.ctrl('z');
        assert_ne!(desk.app.editor.text(), original);
    }
}

#[test]
fn accounts_storage_and_package_views_are_read_only_and_do_not_offer_fake_cloud_or_recovery() {
    use writingdeck_sys::Simulated;
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut desk = Desk::open(
            book(&format!("native-service-availability-{width}"), true),
            width,
            height,
        );
        desk.app
            .deck
            .use_test_backend(Box::new(Simulated::laptop()));
        desk.app.open_settings();
        for _ in 0..3 {
            desk.key(KeyCode::Down);
        }
        desk.key(KeyCode::Enter);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::LocalAccounts);
        assert!(desk.shows("UID 1000"));
        desk.key(KeyCode::Esc);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::GoogleAccount);
        assert!(desk.shows("Google Drive is not connected"));
        desk.key(KeyCode::Enter);
        assert!(!desk.app.deck.busy);
        for route in [Route::GoogleDrive, Route::SyncPanel, Route::RecoveryView] {
            desk.app.deck.shell.nav.reset(route);
            desk.draw();
            assert!(!desk.app.deck.busy);
            assert!(!desk.shows("Connected via OAuth"));
            assert!(!desk.shows("Healthy"));
            // Opening the view alone must not enroll or transmit anything.
            assert!(!desk.app.deck.busy);
        }
        system_panel(&mut desk, 3);
        wait_system(&mut desk);
        desk.choose("Mounted filesystems");
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::StorageDevicesView);
        assert!(desk.shows("MiB free"));
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        desk.key(KeyCode::Home);
        desk.choose("Preview cached package plan");
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::UpdatesView);
        assert!(desk.shows("No package download"));
        assert!(
            desk.app
                .deck
                .shell
                .updates
                .as_ref()
                .unwrap()
                .status
                .contains("SIMULATED")
        );
    }
}

#[test]
fn native_timezone_filter_cancel_confirmation_and_failure_keep_the_original_editor() {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use writingdeck_sys::{Backend, Reply, Request, Simulated};
    struct TimeBackend {
        calls: Arc<Mutex<Vec<Request>>>,
        fail: Arc<AtomicBool>,
        simulator: Simulated,
    }
    impl Backend for TimeBackend {
        fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
            self.calls.lock().unwrap().push(request.clone());
            if matches!(request, Request::SetTimezone(_)) && self.fail.load(Ordering::Relaxed) {
                anyhow::bail!("timezone authorization unavailable");
            }
            self.simulator.request(request)
        }
    }
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut desk = Desk::open(book(&format!("timezone-{width}"), true), width, height);
        let calls = Arc::new(Mutex::new(vec![]));
        let fail = Arc::new(AtomicBool::new(false));
        desk.app.deck.use_test_backend(Box::new(TimeBackend {
            calls: calls.clone(),
            fail: fail.clone(),
            simulator: Simulated::laptop(),
        }));
        desk.app.focus = Focus::Editor;
        desk.typed("Original timezone context. ");
        let original = desk.app.editor.text();
        let position = desk.app.view_position(Pane::Primary);
        system_panel(&mut desk, 4);
        wait_system(&mut desk);
        desk.choose("Choose timezone");
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::Timezones);
        desk.typed("tokyo");
        assert!(desk.shows("Asia/Tokyo"));
        desk.key(KeyCode::Enter);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::TimezoneConfirm);
        assert!(desk.shows("Cancel timezone change"));
        desk.key(KeyCode::Enter); // Cancel is the default.
        assert!(
            !calls
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, Request::SetTimezone(_)))
        );
        desk.key(KeyCode::Enter);
        desk.key(KeyCode::Down);
        fail.store(true, Ordering::Relaxed);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert!(desk.shows("timezone authorization unavailable"));
        assert_eq!(desk.app.deck.shell.nav.current(), Route::TimezoneConfirm);
        assert_eq!(
            desk.app.deck.snapshot.as_ref().unwrap().timezone.as_deref(),
            Some("America/Los_Angeles")
        );
        fail.store(false, Ordering::Relaxed);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(
            desk.app.deck.shell.nav.current(),
            Route::Panel(crate::app::shell::Panel::DateTime)
        );
        assert_eq!(
            desk.app.deck.snapshot.as_ref().unwrap().timezone.as_deref(),
            Some("Asia/Tokyo")
        );
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary), position);
        close_settings(&mut desk);
        desk.ctrl('z');
        assert_ne!(desk.app.editor.text(), original);
    }
}
fn open_warm(desk: &mut Desk) {
    system_panel(desk, 1);
    wait_system(desk);
    desk.key(KeyCode::End);
    desk.key(KeyCode::Enter);
    wait_system(desk);
    assert_eq!(desk.app.deck.shell.nav.current(), Route::WarmScreen);
}
/// Choose the option at `row` of the screen's rows (descriptions included).
fn choose_warm_row(desk: &mut Desk, row: usize) {
    desk.app.deck.shell.nav.select(row, 64);
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
        // Native Power: unavailable Suspend is a description the cursor never reaches.
        desk.key(KeyCode::Home);
        for _ in 0..12 {
            assert!(!desk.shows("▸ Suspend unavailable"));
            desk.key(KeyCode::Down);
        }
        assert_eq!(desk.app.overlay, Overlay::Deck(Page::Settings));
        assert!(desk.shows("  Suspend unavailable: secure screen locking"));
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
fn console_lock_keeps_the_original_context_and_refuses_unsaved_conflicts() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("console-lock-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.typed("Retain both editors through independent locking. ");
        let original = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary);
        let secondary = desk.app.view_position(Pane::Secondary);
        desk.app.deck.lock_available = true; // Only the request flag; no helper is run in a UI test.
        desk.app.request_console_lock();
        assert!(desk.app.deck.lock_requested);
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary), secondary);
        desk.app.deck.lock_requested = false;
        desk.typed("New edits with a conflicted original. ");
        fs::write(&note, "External original remains authoritative.\n").unwrap();
        desk.app.request_console_lock();
        assert!(!desk.app.deck.lock_requested);
        assert_eq!(
            fs::read_to_string(&note).unwrap(),
            "External original remains authoritative.\n"
        );
        assert!(
            desk.app
                .deck
                .shell
                .notices
                .latest()
                .unwrap()
                .contains("unsaved buffers or source conflicts")
        );
        assert!(
            fs::read_dir(note.parent().unwrap())
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| fs::read_to_string(entry.path())
                    .is_ok_and(|text| text.contains("New edits with a conflicted original.")))
        );
    }
}

#[test]
fn live_session_uses_shared_surfaces_and_never_requests_personal_lock() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("live-session-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        let original = desk.app.editor.text();
        let position = desk.app.view_position(Pane::Secondary);
        desk.app.deck.live_session = true;
        desk.app.deck.lock_available = true; // A stale capability must not enable a personal lock.
        desk.key(KeyCode::F(12));
        desk.draw();
        assert!(desk.shows("WrOS · Live session"));
        desk.key(KeyCode::Esc);
        system_panel(&mut desk, 2);
        desk.draw();
        assert!(desk.shows("Live session"));
        assert!(desk.shows("personal lock unavailable"));
        desk.app.request_console_lock();
        assert!(!desk.app.deck.lock_requested);
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Secondary), position);
    }
}

#[test]
fn live_media_opens_on_the_dashboard_with_install_chosen_and_confirms_before_installing() {
    for (width, height) in [(80, 24), (160, 50)] {
        let mut desk = Desk::open(
            book(&format!("live-install-row-{width}"), true),
            width,
            height,
        );
        // An installed system's dashboard has no install row and keeps its action order.
        desk.key(KeyCode::F(12));
        desk.draw();
        assert!(!desk.shows("Install WrOS to disk"));
        desk.key(KeyCode::Esc);

        desk.app.deck.live_session = true;
        desk.app.deck.installer_available = true;
        desk.app.overlay = Overlay::None;
        desk.app.init_writingdeck(true);
        desk.draw();
        assert!(desk.shows("Try WrOS here, or choose Install WrOS to disk"));
        assert!(desk.shows("▸ Install WrOS to disk"));

        // Enter opens the installer confirmation, whose default is to stay in the live session.
        desk.key(KeyCode::Enter);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::InstallerConfirm);
        desk.key(KeyCode::Enter);
        assert!(!desk.app.deck.installer_requested);

        // Without an installer on this console the row is absent, as in Settings > Storage.
        desk.app.deck.installer_available = false;
        desk.app.overlay = Overlay::None;
        desk.key(KeyCode::F(12));
        desk.draw();
        assert!(!desk.shows("Install WrOS to disk"));
    }
}

#[test]
fn settings_descriptions_are_never_chosen_and_options_show_where_they_lead() {
    let mut desk = Desk::open(book("menu-row-kinds", true), 100, 30);
    desk.app.open_settings();
    desk.draw();
    assert!(desk.shows("▸ Writing ›"), "{}", desk.rows().join("\n"));

    // Backups opens with a description first: the cursor starts on the first option.
    desk.app.deck.shell.nav.open(Route::Backups);
    desk.draw();
    assert!(desk.shows("    No backup operation is running"));
    assert!(!desk.shows("▸ No backup operation is running"));
    desk.key(KeyCode::Up);
    assert!(!desk.shows("▸ No backup operation is running"));

    // Confirmations keep Cancel first; End and Down stop on the last option.
    desk.app.deck.live_session = true;
    desk.app.deck.installer_available = true;
    desk.app.overlay = Overlay::None;
    desk.app.open_installer_confirm();
    desk.draw();
    assert!(desk.shows("▸ Cancel and remain in the live writing session"));
    desk.key(KeyCode::End);
    desk.key(KeyCode::Down);
    assert!(desk.shows("▸ Start encrypted WrOS installer on console tty7"));
    assert!(desk.shows("    Retires writing input on tty1"));
    desk.key(KeyCode::Home);
    desk.key(KeyCode::Enter);
    assert!(!desk.app.deck.installer_requested);
}

fn online_backend(status: &str, version: Option<&str>) -> writingdeck_sys::Simulated {
    let mut sim = writingdeck_sys::Simulated::laptop();
    sim.online = Some(writingdeck_sys::OnlineUpdate {
        status: status.into(),
        version: version.map(Into::into),
        installed: Some("0.1.0~rc2".into()),
        clock: "2001-01-01 00:00".into(),
    });
    sim.fetched = Some("/var/cache/wros-update/online/0123456789abcdef".into());
    sim
}

#[test]
fn online_update_available_alerts_once_and_update_opens_the_existing_review() {
    let mut desk = Desk::open(book("online-update", true), 100, 30);
    desk.app
        .deck
        .use_test_backend(Box::new(online_backend("available", Some("0.1.0~rc3"))));
    // Only an installed appliance session checks; tests and the live session never do.
    desk.app.tick_deck();
    assert!(!desk.app.deck.busy);
    desk.app.deck.online_updates = true;
    // The first check waits for Wi-Fi to come up after login.
    desk.app.tick_deck();
    assert!(
        !desk.app.deck.busy,
        "no check the moment the session starts"
    );
    let first = desk.app.deck.shell.update_flow.online_next.unwrap();
    let wait = first.duration_since(std::time::SystemTime::now()).unwrap();
    assert!(
        wait > std::time::Duration::from_secs(60) && wait <= std::time::Duration::from_secs(120)
    );
    // Never while a keyboard or display preview waits for confirmation.
    desk.app.deck.shell.update_flow.online_next = Some(std::time::SystemTime::now());
    desk.app.open_settings();
    desk.app.deck.shell.warm_preview = Some((
        "token".into(),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    ));
    desk.app.tick_deck();
    assert!(!desk.app.deck.busy, "no check during a preview");
    desk.app.deck.shell.warm_preview = None;
    desk.app.overlay = Overlay::None;
    desk.app.tick_deck();
    wait_system(&mut desk);
    assert_eq!(
        desk.app.msg,
        "WrOS 0.1.0~rc3 is available · Settings › Updates"
    );
    assert!(desk.app.deck.note.ends_with("WrOS 0.1.0~rc3 is available"));
    desk.app.msg.clear();
    desk.app.tick_deck();
    assert!(!desk.app.deck.busy, "checks at most once a day");
    let next = desk.app.deck.shell.update_flow.online_next.unwrap();
    let wait = next.duration_since(std::time::SystemTime::now()).unwrap();
    assert!(wait > std::time::Duration::from_secs(23 * 3600));
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    assert!(desk.shows("    WrOS 0.1.0~rc3 is available"));
    desk.key(KeyCode::Home);
    desk.choose("Update WrOS to 0.1.0~rc3");
    wait_system(&mut desk);
    wait_system(&mut desk);
    assert_eq!(route(&desk), Route::UpdateReview);
}

#[test]
fn online_update_clock_and_offline_states_never_alert() {
    for (status, text) in [
        (
            "clock",
            "Couldn't check for WrOS updates: this computer's clock may be wrong (it reads 2001-01-01 00:00).",
        ),
        ("unreachable", "Couldn't reach the WrOS update server"),
        ("unpublished", "No WrOS updates have been published yet"),
        (
            "expired",
            "The WrOS update server's signature has expired; WrOS will check again later",
        ),
        ("current", "WrOS is up to date (0.1.0~rc2)"),
    ] {
        let mut desk = Desk::open(book(&format!("online-{status}"), true), 140, 30);
        desk.app
            .deck
            .use_test_backend(Box::new(online_backend(status, None)));
        desk.app.deck.online_updates = true;
        desk.app.deck.shell.update_flow.online_next = Some(std::time::SystemTime::now());
        desk.app.tick_deck();
        wait_system(&mut desk);
        assert!(!desk.app.msg.contains("is available"), "{status}");
        // A failed check tries again within the hour, not tomorrow.
        let wait = desk
            .app
            .deck
            .shell
            .update_flow
            .online_next
            .unwrap()
            .duration_since(std::time::SystemTime::now())
            .unwrap();
        if status == "current" {
            assert!(wait > std::time::Duration::from_secs(23 * 3600));
        } else {
            assert!(wait <= std::time::Duration::from_secs(3600), "{status}");
        }
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        assert!(desk.shows(text), "{status}: {}", desk.rows().join("\n"));
        assert!(!desk.shows("Update WrOS to"));
    }
}

/// A stand-in wros-sync: records each request, answers from `<dir>/<operation>.json`,
/// and on enable writes the book's sync identity as the real helper does.
fn sync_stub(desk: &mut Desk, sync_reply: &str) -> std::path::PathBuf {
    let dir = desk.root.join(".sync-stub");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("wros-sync"),
        "import json, os, sys\nfrom pathlib import Path\n\
         request = json.load(sys.stdin)\nhere = Path(os.environ['STUB_DIR'])\n\
         with open(here / 'log', 'a') as log:\n    log.write(json.dumps(request) + '\\n')\n\
         if request['operation'] == 'enable':\n\
         \x20   (Path(request['project']) / '.grimoire').mkdir(exist_ok=True)\n\
         \x20   (Path(request['project']) / '.grimoire/sync.json').write_text('{}')\n\
         print((here / (request['operation'] + '.json')).read_text())\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("enable.json"),
        r#"{"ok":true,"data":{"status":"enabled","message":"Syncing.","drive_folder":"Book"}}"#,
    )
    .unwrap();
    std::fs::write(dir.join("sync.json"), sync_reply).unwrap();
    let sync = &mut desk.app.deck.shell.backup.sync;
    sync.script = Some(dir.join("wros-sync"));
    sync.env = vec![("STUB_DIR", dir.clone())];
    desk.app.deck.shell.backup.status.google_connected = true;
    dir
}

fn wait_sync(desk: &mut Desk) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        desk.app.tick_deck();
        let sync = &desk.app.deck.shell.backup.sync;
        if (sync.job.is_none() && sync.running.is_none()) || std::time::Instant::now() > end {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    desk.draw();
}

fn sync_log(dir: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("log"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn switching_sync_on_enables_the_book_then_syncs_and_shows_the_result() {
    let mut desk = Desk::open(book("sync-on", true), 120, 40);
    let dir = sync_stub(
        &mut desk,
        r#"{"ok":true,"data":{"status":"synced","uploaded":1,"downloaded":2,"deleted":0,"message":"Synced."}}"#,
    );
    desk.app.open_settings();
    desk.app.deck.shell.nav.open(Route::Backups);
    desk.draw();
    assert!(desk.shows("Readable copies in Drive are not encrypted"));
    desk.key(KeyCode::Home);
    desk.choose("Sync this book with Google Drive (readable files)");
    wait_sync(&mut desk);
    wait_sync(&mut desk);
    let ops: Vec<_> = sync_log(&dir)
        .iter()
        .map(|r| r["operation"].clone())
        .collect();
    assert_eq!(ops, ["enable", "sync"]);
    assert!(desk.shows("    Synced "), "{}", desk.rows().join("\n"));
    assert!(desk.shows("1 up, 2 down, 0 deleted"));
    assert!(desk.shows("Stop syncing this book"));
    let next = desk.app.deck.shell.backup.sync.next.unwrap();
    assert!(next > std::time::Instant::now() + std::time::Duration::from_secs(4 * 60));
}

#[test]
fn a_save_brings_the_next_sync_forward_and_the_delete_guard_asks_first() {
    let mut desk = Desk::open(book("sync-guard", true), 120, 40);
    let dir = sync_stub(
        &mut desk,
        r#"{"ok":true,"data":{"status":"confirm","deletes":7,"message":"Stopped: this sync would delete 7 files."}}"#,
    );
    std::fs::create_dir_all(desk.root.join(".grimoire")).unwrap();
    std::fs::write(desk.root.join(".grimoire/sync.json"), "{}").unwrap();
    // Opening a book that syncs runs a sync straight away.
    wait_sync(&mut desk);
    wait_sync(&mut desk);
    assert_eq!(sync_log(&dir).len(), 1);
    desk.app.open_settings();
    desk.app.deck.shell.nav.open(Route::Backups);
    desk.draw();
    assert!(desk.shows("    Stopped: this sync would delete 7 files"));
    desk.key(KeyCode::Home);
    desk.choose("Sync and delete 7 files");
    wait_sync(&mut desk);
    assert_eq!(sync_log(&dir).last().unwrap()["allow_deletes"], true);
    // A save (unsaved words, then none) brings the next sync to within ten seconds.
    let sync = &mut desk.app.deck.shell.backup.sync;
    sync.next = Some(std::time::Instant::now() + std::time::Duration::from_secs(300));
    sync.saw_dirty = true;
    desk.app.tick_deck();
    let next = desk.app.deck.shell.backup.sync.next.unwrap();
    assert!(next <= std::time::Instant::now() + std::time::Duration::from_secs(10));
}

/// A scene changed in WrOS and in Drive since `base`, with Drive's version parked
/// beside it by a sync, as `wros-sync` leaves it. Returns (scene, parked copy).
fn parked_drive_edit(
    root: &std::path::Path,
    base: &str,
    mine: &str,
    theirs: &str,
) -> (
    std::path::PathBuf,
    std::path::PathBuf,
    crate::sync_service::Pending,
) {
    let scene = first_scene(root);
    std::fs::write(&scene, mine).unwrap();
    let stem = scene.file_stem().unwrap().to_string_lossy().into_owned();
    let parked = scene.with_file_name(format!("{stem} (from Drive, 2026-11-02 09-30).md"));
    std::fs::write(&parked, theirs).unwrap();
    let merge_base = root.join(".sync-merge-base.md");
    std::fs::write(&merge_base, base).unwrap();
    let relative =
        |p: &std::path::Path| p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
    let pending = crate::sync_service::Pending {
        path: relative(&scene),
        parked: relative(&parked),
        merge_base: Some(merge_base.to_string_lossy().into_owned()),
    };
    (scene, parked, pending)
}

#[test]
fn a_drive_edit_to_other_paragraphs_merges_itself_without_moving_the_screen() {
    let root = book("sync-automerge", true);
    let (scene, parked, pending) = parked_drive_edit(
        &root,
        "---\ntitle: Scene One\n---\n\nFirst.\n\nSecond.\n",
        "---\ntitle: Scene One\n---\n\nFirst, mine.\n\nSecond.\n",
        "---\ntitle: Scene One\n---\n\nFirst.\n\nSecond, theirs.\n",
    );
    let mut desk = Desk::open(root, 120, 40);
    desk.app.open_settings();
    desk.app.deck.shell.backup.sync.pending = vec![pending];
    desk.app.tick_deck();
    desk.draw();
    assert_eq!(
        std::fs::read_to_string(&scene).unwrap(),
        "---\ntitle: Scene One\n---\n\nFirst, mine.\n\nSecond, theirs.\n",
        "{}",
        desk.app.msg
    );
    assert!(!parked.exists(), "the parked copy goes to the trash");
    assert!(
        desk.app.msg.contains("merged Drive's edits"),
        "{}",
        desk.app.msg
    );
    assert_eq!(desk.app.overlay, Overlay::Deck(Page::Settings));
    assert!(desk.app.deck.shell.backup.sync.pending.is_empty());
}

#[test]
fn a_clash_waits_for_the_writer_and_merges_paragraph_by_paragraph() {
    let root = book("sync-clash", true);
    let (scene, parked, pending) = parked_drive_edit(
        &root,
        "---\ntitle: Scene One\n---\n\nFirst.\n\nSecond.\n",
        "---\ntitle: Scene One\n---\n\nFirst, mine.\n\nSecond.\n",
        "---\ntitle: Scene One\n---\n\nFirst, theirs.\n\nSecond, theirs.\n",
    );
    let mut desk = Desk::open(root, 120, 40);
    desk.app.deck.shell.backup.sync.pending = vec![pending];
    desk.app.tick_deck();
    assert!(parked.exists(), "a clash never settles itself");
    assert!(desk.app.msg.contains("needs merging"), "{}", desk.app.msg);
    desk.app.open_conflicts();
    desk.key(KeyCode::Enter);
    desk.draw();
    assert!(desk.shows("Merge paragraph by paragraph"));
    desk.key(KeyCode::Char('4'));
    desk.draw();
    assert!(desk.shows("Clash 1 of 1"), "{}", desk.rows().join("\n"));
    assert!(desk.shows("First, mine.") && desk.shows("First, theirs."));
    desk.key(KeyCode::Char('3'));
    assert_eq!(
        std::fs::read_to_string(&scene).unwrap(),
        "---\ntitle: Scene One\n---\n\nFirst, mine.\n\nFirst, theirs.\n\nSecond, theirs.\n"
    );
    assert!(!parked.exists());
    desk.ctrl('z');
    assert_eq!(
        std::fs::read_to_string(&scene).unwrap(),
        "---\ntitle: Scene One\n---\n\nFirst, mine.\n\nSecond.\n"
    );
}

#[test]
fn a_manual_drive_merge_keeps_unsaved_words_when_the_writer_chooses_mine() {
    let root = book("sync-merge-unsaved", true);
    let (scene, parked, pending) = parked_drive_edit(
        &root,
        "---\ntitle: Scene One\n---\n\nFirst.\n\nSecond.\n",
        "---\ntitle: Scene One\n---\n\nFirst, mine.\n\nSecond.\n",
        "---\ntitle: Scene One\n---\n\nFirst, theirs.\n\nSecond, theirs.\n",
    );
    let mut desk = Desk::open(root, 120, 40);
    desk.app.deck.shell.backup.sync.pending = vec![pending];
    // The conflict screen opens the original scene, even when the parked copy
    // sorted first on launch. Return to it and type before autosave runs.
    desk.app.open_conflicts();
    desk.key(KeyCode::Enter);
    desk.key(KeyCode::Esc);
    desk.key(KeyCode::Esc);
    assert_eq!(
        desk.app
            .open
            .map(|i| desk.app.project.nodes[i].path.clone()),
        Some(scene.clone())
    );
    desk.app.focus = Focus::Editor;
    desk.typed("Unsaved words. ");
    assert!(desk.app.project.dirty_count() > 0);
    assert!(
        !std::fs::read_to_string(&scene)
            .unwrap()
            .contains("Unsaved words.")
    );
    desk.app.open_conflicts();
    desk.key(KeyCode::Enter);
    desk.key(KeyCode::Char('4'));
    assert!(desk.shows("Unsaved words."), "{}", desk.rows().join("\n"));
    desk.key(KeyCode::Char('1'));
    assert_eq!(
        std::fs::read_to_string(&scene).unwrap(),
        "---\ntitle: Scene One\n---\n\nUnsaved words. First, mine.\n\nSecond, theirs.\n"
    );
    assert!(!parked.exists());
    desk.app.focus = Focus::Tree;
    desk.ctrl('z');
    assert_eq!(
        std::fs::read_to_string(&scene).unwrap(),
        "---\ntitle: Scene One\n---\n\nUnsaved words. First, mine.\n\nSecond.\n"
    );
}

#[test]
fn update_wros_when_already_current_says_so_without_network_advice() {
    let mut desk = Desk::open(book("online-already-current", true), 120, 30);
    let mut sim = online_backend("available", Some("0.1.0~rc3"));
    sim.fetched = None;
    sim.fetch_error = Some("WrOS is already up to date (0.1.0~rc3)".into());
    desk.app.deck.use_test_backend(Box::new(sim));
    desk.app.deck.online_updates = true;
    desk.app.deck.shell.update_flow.online_next = Some(std::time::SystemTime::now());
    desk.app.tick_deck();
    wait_system(&mut desk);
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    desk.key(KeyCode::Home);
    desk.choose("Update WrOS to 0.1.0~rc3");
    wait_system(&mut desk);
    assert!(
        desk.shows("WrOS is up to date"),
        "{}",
        desk.rows().join("\n")
    );
    assert!(!desk.shows("Update WrOS to"));
    assert!(!desk.shows("Check the network"));
}

#[test]
fn date_time_panel_says_whether_the_clock_is_set_from_the_network() {
    let mut desk = Desk::open(book("network-time", true), 100, 30);
    desk.app.deck.simulate();
    system_panel(&mut desk, 4);
    wait_system(&mut desk);
    assert!(desk.shows("    Network time: on · not yet synchronized"));
    assert!(!desk.shows("not available yet"));
    for (ntp, synced, text) in [
        (Some(true), Some(true), "Network time: on · synchronized"),
        (Some(false), Some(false), "Network time: off"),
        (None, None, "Network time: unknown"),
    ] {
        let snapshot = desk.app.deck.snapshot.as_mut().unwrap();
        snapshot.network_time = ntp;
        snapshot.time_synchronized = synced;
        desk.draw();
        assert!(desk.shows(text), "{text}");
    }
}

#[test]
fn a_running_backup_shows_its_step_percentage_and_time_under_cancel() {
    use crate::backup_service::{Job, Progress};
    let mut desk = Desk::open(book("backup-progress", true), 100, 30);
    desk.app.open_settings();
    desk.app.deck.shell.nav.open(Route::Backups);
    desk.app.deck.shell.backup.job = Some(Job::running_for_test(Some(Progress {
        phase: "Encrypting and saving the snapshot".into(),
        percent: Some(0.42),
        offer: None,
    })));
    desk.draw();
    assert!(desk.shows("▸ Cancel current backup operation"));
    assert!(desk.shows("    Working: Encrypting and saving the snapshot · 42% · 0 s"));
    desk.app.deck.shell.backup.job = Some(Job::running_for_test(None));
    desk.draw();
    assert!(desk.shows("    Working: starting · 0 s"));
    desk.app.deck.shell.backup.job = None;
    desk.draw();
    assert!(!desk.shows("Working:"));
    assert!(desk.shows("    No snapshots listed yet; refresh snapshots to check"));
    let status = &mut desk.app.deck.shell.backup.status;
    status.snapshots.push(crate::backup_service::Snapshot {
        id: "a".repeat(64),
        time: "2026-11-01T11:26:50.59245339-08:00".into(),
        _path: "/book".into(),
        project: "Book".into(),
        restore_checked_at: String::new(),
        has_profile: false,
    });
    status.queued_backups.push(Default::default());
    desk.draw();
    assert!(desk.shows("    Newest snapshot: 2026-11-01 11:26 · 1 queued backup waiting"));
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

#[test]
fn native_wifi_enrollment_masks_credentials_handles_cancel_failure_and_success_preserving_both_editors()
 {
    use std::sync::{Arc, Mutex};
    use writingdeck_sys::{Backend, Reply, Request, Simulated};
    struct RecordedWifi(Arc<Mutex<Vec<Request>>>, Simulated);
    impl Backend for RecordedWifi {
        fn simulated(&self) -> bool {
            true
        }
        fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
            self.0.lock().unwrap().push(request.clone());
            self.1.request(request)
        }
    }
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) = secondary_book(&format!("wifi-enroll-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.typed("Keep both editors during Wi-Fi enrollment. ");
        desk.app
            .with_editor_view(Pane::Primary, |app| app.editor.select((11, 5), (8, 3)));
        desk.app.editor.select((9, 2), (6, 4));
        desk.draw();
        let original = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary).unwrap();
        let secondary = desk.app.view_position(Pane::Secondary).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        desk.app
            .deck
            .use_test_backend(Box::new(RecordedWifi(calls.clone(), Simulated::laptop())));

        system_panel(&mut desk, 0);
        wait_system(&mut desk);
        desk.key(KeyCode::Home);
        desk.choose("Scan Wi-Fi networks");
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiNetworks);

        // Select WriterDeck-Cafe (row 2).
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        desk.draw();
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiConnect);
        assert!(desk.shows("WriterDeck-Cafe"));

        // Paste and clipboard-paste secret into masked field; never rendered or leaked to editor.
        desk.app.paste("PRIVATE_WIFI_SECRET_A");
        desk.draw();
        assert!(desk.shows("[hidden]"));
        assert!(!desk.rows().join("\n").contains("PRIVATE_WIFI_SECRET_A"));
        desk.app.clip = "_CLIP_SECRET_B".into();
        desk.ctrl('v');
        desk.draw();
        assert!(!desk.rows().join("\n").contains("_CLIP_SECRET_B"));
        assert_eq!(desk.app.editor.text(), original);

        // Escape cancels and discards the password without calling ConnectWifi.
        desk.key(KeyCode::Esc);
        assert!(desk.app.deck.shell.wifi_form.is_none());
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiNetworks);
        assert!(
            !calls
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, Request::ConnectWifi { .. }))
        );

        // Reopen WriterDeck-Cafe: test too-short password refusal before worker call.
        desk.key(KeyCode::Enter);
        desk.typed("short");
        desk.key(KeyCode::Enter); // advances selection to Connect
        desk.key(KeyCode::Enter); // submits
        desk.draw();
        assert!(desk.shows("8–63 bytes"));
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiConnect);
        assert!(
            !calls
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, Request::ConnectWifi { .. }))
        );

        // Select password field, clear it, enter wrong-password, and verify failure recovery.
        desk.key(KeyCode::Home);
        desk.key(KeyCode::Delete);
        desk.typed("wrong-password");
        desk.key(KeyCode::Enter);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiConnect);
        assert!(desk.shows("authentication failed"));
        assert!(
            desk.app
                .deck
                .shell
                .wifi_form
                .as_ref()
                .unwrap()
                .password
                .is_empty(),
            "failed attempt clears the secret buffer for retry"
        );
        for req in calls.lock().unwrap().iter() {
            assert!(!format!("{req:?}").contains("wrong-password"));
            assert!(
                !serde_json::to_string(req)
                    .unwrap()
                    .contains("wrong-password")
            );
        }

        // Enter valid password and connect.
        desk.key(KeyCode::Home);
        desk.typed("correct-horse-battery");
        desk.key(KeyCode::Enter);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiNetworks);
        assert!(desk.shows("Connected: WriterDeck-Cafe"));
        for req in calls.lock().unwrap().iter() {
            assert!(!format!("{req:?}").contains("correct-horse-battery"));
            assert!(
                !serde_json::to_string(req)
                    .unwrap()
                    .contains("correct-horse-battery")
            );
        }

        // Connect to open network PublicLibrary without a password field.
        desk.key(KeyCode::Home);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiConnect);
        desk.key(KeyCode::Down); // move from default Cancel to Connect
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::WifiNetworks);
        assert!(desk.shows("Connected: PublicLibrary"));

        // Return to Network panel: disconnect and manage saved profiles.
        desk.key(KeyCode::Esc);
        desk.key(KeyCode::Home);
        desk.choose("Disconnect active Wi-Fi");
        wait_system(&mut desk);
        assert!(desk.shows("Wi-Fi disconnected"));

        // Open Saved Wi-Fi profiles.
        desk.key(KeyCode::Home);
        desk.choose("Saved Wi-Fi profiles");
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::SavedWifi);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::SavedWifiProfile);
        desk.key(KeyCode::Enter); // Default is Cancel.
        assert_eq!(desk.app.deck.shell.nav.current(), Route::SavedWifi);

        // Reconnect saved profile by UUID.
        desk.key(KeyCode::Enter);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::SavedWifi);
        assert!(calls.lock().unwrap().contains(&Request::ActivateSavedWifi(
            "00000000-0000-4000-8000-000000000001".into()
        )));

        // Forget saved profile by UUID.
        desk.key(KeyCode::Enter);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert!(calls.lock().unwrap().contains(&Request::ForgetWifi(
            "00000000-0000-4000-8000-000000000001".into()
        )));

        for _ in 0..4 {
            desk.key(KeyCode::Esc);
        }
        assert_eq!(desk.app.overlay, Overlay::None);
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary).unwrap(), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary).unwrap(), secondary);
        assert_eq!(desk.app.focus, Focus::Secondary);
        desk.ctrl('z');
        assert_ne!(desk.app.editor.text(), original);
    }
}

// ---- offline update review, staging, cancel and repair ---------------------

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use writingdeck_sys::{Backend, Reply, Request, Simulated, UpdateState};

const MEDIA: &str = "/media/usb/wros-signed-updates";

/// A simulated update helper shared with the test, recording every request.
/// The flags make the "helper" lie or fail so the UI's read-back is observable.
#[derive(Clone)]
struct UpdateWorld {
    sim: Arc<Mutex<Simulated>>,
    calls: Arc<Mutex<Vec<Request>>>,
    fail_stage: Arc<AtomicBool>,
    cancel_is_noop: Arc<AtomicBool>,
    repair_is_noop: Arc<AtomicBool>,
}
struct WorldBackend(UpdateWorld);
impl Backend for WorldBackend {
    fn simulated(&self) -> bool {
        true
    }
    fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
        let world = &self.0;
        world.calls.lock().unwrap().push(request.clone());
        match &request {
            Request::StageUpdates { .. } if world.fail_stage.load(Ordering::Relaxed) => {
                anyhow::bail!("the helper could not write its journal")
            }
            Request::CancelStagedUpdates if world.cancel_is_noop.load(Ordering::Relaxed) => {
                return Ok(Reply::Message("Cancelled (helper claim).".into()));
            }
            Request::RecoverUpdates if world.repair_is_noop.load(Ordering::Relaxed) => {
                return Ok(Reply::Message("Repaired (helper claim).".into()));
            }
            _ => {}
        }
        world.sim.lock().unwrap().request(request)
    }
}
impl UpdateWorld {
    fn install(desk: &mut Desk) -> Self {
        let world = Self {
            sim: Arc::new(Mutex::new(Simulated::laptop())),
            calls: Arc::default(),
            fail_stage: Arc::default(),
            cancel_is_noop: Arc::default(),
            repair_is_noop: Arc::default(),
        };
        desk.app
            .deck
            .use_test_backend(Box::new(WorldBackend(world.clone())));
        world
    }
    fn calls(&self) -> Vec<Request> {
        self.calls.lock().unwrap().clone()
    }
    fn count(&self, wanted: impl Fn(&Request) -> bool) -> usize {
        self.calls().iter().filter(|r| wanted(r)).count()
    }
    /// Requests that change the system: none may precede an explicit confirmation.
    fn mutations(&self) -> usize {
        self.count(|r| {
            matches!(
                r,
                Request::StageUpdates { .. }
                    | Request::CancelStagedUpdates
                    | Request::RecoverUpdates
                    | Request::GoBackToPreviousVersion
                    | Request::Power(_)
            )
        })
    }
    fn stages(&self) -> Vec<(String, String)> {
        self.calls()
            .into_iter()
            .filter_map(|r| match r {
                Request::StageUpdates {
                    bundle_dir,
                    review_id,
                } => Some((bundle_dir, review_id)),
                _ => None,
            })
            .collect()
    }
    fn state(&self) -> UpdateState {
        self.sim.lock().unwrap().recovery.update_state
    }
}

fn wait_backup(desk: &mut Desk) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while desk.app.deck.shell.backup.job.is_some() && std::time::Instant::now() < end {
        desk.app.deck.shell.backup.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(desk.app.deck.shell.backup.job.is_none());
    desk.draw();
}

/// Replace the backup service's answer with a known one, as if just read.
fn set_backup(desk: &mut Desk, configured: bool, fresh: bool) {
    let root = desk.app.project.root.to_string_lossy().into_owned();
    let backup = &mut desk.app.deck.shell.backup;
    backup.status.configured = configured;
    backup.status_ok = true;
    backup.status.snapshots = if fresh {
        vec![crate::backup_service::Snapshot {
            id: "a".repeat(64),
            time: chrono::Utc::now().to_rfc3339(),
            _path: root,
            project: "Book".into(),
            restore_checked_at: String::new(),
            has_profile: false,
        }]
    } else {
        vec![]
    };
    desk.draw();
}

fn review_by_path(desk: &mut Desk, path: &str) {
    desk.choose("Review a signed update bundle by path");
    assert_eq!(desk.app.deck.shell.nav.current(), Route::UpdateBundleForm);
    desk.app.paste(path);
    desk.key(KeyCode::Enter);
    desk.key(KeyCode::Enter);
    wait_system(desk);
}

fn reach_confirm(desk: &mut Desk) {
    review_by_path(desk, MEDIA);
    assert_eq!(desk.app.deck.shell.nav.current(), Route::UpdateReview);
    desk.choose("Continue to confirmation");
    assert_eq!(desk.app.deck.shell.nav.current(), Route::UpdateConfirm);
    wait_backup(desk);
}

fn route(desk: &Desk) -> Route {
    desk.app.deck.shell.nav.current()
}

/// Escape out of Settings entirely, whatever screen it is on.
fn close_all(desk: &mut Desk) {
    for _ in 0..10 {
        if desk.app.overlay == Overlay::None {
            return;
        }
        desk.key(KeyCode::Esc);
    }
    assert_eq!(desk.app.overlay, Overlay::None);
}

const SIZES: [(u16, u16); 4] = [(80, 24), (100, 30), (120, 40), (160, 50)];

#[test]
fn update_review_lists_signer_expiry_and_exact_changes_and_installs_nothing_at_four_sizes() {
    for (width, height) in SIZES {
        let (mut desk, note) =
            secondary_book(&format!("update-review-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        let world = UpdateWorld::install(&mut desk);
        desk.typed("Primary update draft. ");
        desk.typed("Secondary update notes. ");
        let original = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary).unwrap();
        let secondary = desk.app.view_position(Pane::Secondary).unwrap();
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        // Opening the panel only reads status.
        assert!(world.calls().iter().all(|r| *r == Request::RecoveryStatus));
        assert!(desk.shows("Update state: idle"));
        assert!(!desk.shows("unavailable pending"));

        // Find media mounts and then reviews; the review lists every change.
        desk.choose("Find update media");
        wait_system(&mut desk);
        assert_eq!(
            route(&desk),
            Route::UpdateReview,
            "{}",
            desk.rows().join("\n")
        );
        let calls = world.calls();
        assert!(calls.contains(&Request::FindUpdateMedia));
        assert!(calls.contains(&Request::ReviewUpdateBundle {
            bundle_dir: "/run/wros-update/media/0".into()
        }));
        assert!(desk.shows("SIMULATED"));
        assert!(desk.shows("Signed by key SIMULATED-NO-KEY-CHECKED"));
        assert!(desk.shows("Valid until 2026-12-31"));
        assert!(desk.shows("Upgrade wros: 0.1.0~rc1 → 0.1.0~rc2"));
        assert!(desk.shows("Install wros-help: not installed → 0.1.0~rc2"));
        assert!(desk.shows("2 package changes"));
        // The safe row is the default and nothing has been staged.
        assert!(desk.shows("▸ Back · do not install anything"));
        assert_eq!(world.mutations(), 0);
        assert_eq!(world.state(), UpdateState::Idle);

        // Backing out drops the review: it must be authenticated again.
        desk.key(KeyCode::Esc);
        assert_eq!(
            route(&desk),
            Route::Panel(crate::app::shell::Panel::Updates)
        );
        assert!(desk.app.deck.shell.update_flow.review.is_none());
        assert_eq!(world.mutations(), 0);

        for _ in 0..3 {
            desk.key(KeyCode::Esc);
        }
        assert_eq!(desk.app.overlay, Overlay::None);
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary).unwrap(), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary).unwrap(), secondary);
        assert_eq!(desk.app.focus, Focus::Secondary);
    }
}

#[test]
fn update_path_entry_refuses_reserved_locations_without_sending_anything() {
    for (width, height) in SIZES {
        let mut desk = Desk::open(book(&format!("update-paths-{width}"), true), width, height);
        let world = UpdateWorld::install(&mut desk);
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        let before = world.calls().len();
        desk.choose("Review a signed update bundle by path");
        for (path, why) in [
            ("relative/bundle", "absolute directory path"),
            ("/etc", "update media"),
            ("/var/lib/wros-update/public", "update media"),
            ("/run/wros-update", "update media"),
            ("/home/writer/novel", "update media"),
            ("/media/../etc", "absolute directory path"),
        ] {
            desk.key(KeyCode::Home);
            desk.key(KeyCode::Delete);
            desk.app.paste(path);
            desk.key(KeyCode::Enter);
            desk.key(KeyCode::Enter);
            assert!(!desk.app.deck.busy, "{path} was sent");
            assert!(
                desk.app.deck.note.contains(why),
                "{path}: {}",
                desk.app.deck.note
            );
            assert_eq!(route(&desk), Route::UpdateBundleForm);
        }
        assert_eq!(
            world.calls().len(),
            before,
            "refused paths reached the worker"
        );
    }
}

#[test]
fn update_confirmation_defaults_to_cancel_and_stages_only_after_every_gate_passes() {
    for (width, height) in SIZES {
        let (mut desk, note) =
            secondary_book(&format!("update-confirm-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        let world = UpdateWorld::install(&mut desk);
        desk.typed("Draft before the update. ");
        let original = desk.app.editor.text();
        system_panel(&mut desk, 5);
        wait_system(&mut desk);

        // Default is Cancel: Enter on the untouched confirmation does nothing.
        reach_confirm(&mut desk);
        assert!(
            desk.shows("▸ Cancel · do not install this update"),
            "{}",
            desk.rows().join("\n")
        );
        assert!(desk.shows("Stage this update"));
        desk.key(KeyCode::Enter);
        assert_eq!(
            route(&desk),
            Route::Panel(crate::app::shell::Panel::Updates)
        );
        assert_eq!(world.mutations(), 0);

        // Backup recommended and configured, but none is fresh: refused.
        reach_confirm(&mut desk);
        set_backup(&mut desk, true, false);
        assert!(desk.shows("Backup: none in the last 24 hours"));
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        assert_eq!(route(&desk), Route::UpdateConfirm);
        assert_eq!(world.mutations(), 0, "staged without a fresh backup");
        assert!(desk.app.deck.note.contains("no backup of this project"));

        // A backup status that was never read is also refused (fail closed).
        desk.app.deck.shell.backup.status_ok = false;
        desk.key(KeyCode::Enter);
        assert_eq!(world.mutations(), 0, "staged on unknown backup status");
        wait_backup(&mut desk);

        // Unsaved edits with a conflicting external change are refused.
        set_backup(&mut desk, true, true);
        assert!(desk.shows("Backup: fresh"));
        close_all(&mut desk);
        desk.typed("Unsaved edit before external conflict. ");
        fs::write(&note, "External file modification.\n").unwrap();
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        reach_confirm(&mut desk);
        set_backup(&mut desk, true, true);
        desk.app.deck.note.clear();
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        assert_eq!(
            world.mutations(),
            0,
            "staged over unsaved or conflicting work"
        );
        assert!(
            desk.app
                .deck
                .note
                .contains("unsaved buffers or source conflicts"),
            "{}",
            desk.app.deck.note
        );
        assert_eq!(route(&desk), Route::UpdateConfirm);
        assert_ne!(desk.app.editor.text(), original);
    }

    // A clean tree with a fresh backup stages exactly what was reviewed.
    for (width, height) in SIZES {
        let (mut desk, note) =
            secondary_book(&format!("update-stage-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        let world = UpdateWorld::install(&mut desk);
        desk.typed("Primary draft kept through staging. ");
        let original = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary).unwrap();
        let secondary = desk.app.view_position(Pane::Secondary).unwrap();
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        reach_confirm(&mut desk);
        let reviewed = desk
            .app
            .deck
            .shell
            .update_flow
            .review
            .clone()
            .unwrap()
            .review_id;
        set_backup(&mut desk, true, true);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(world.stages(), [(MEDIA.to_string(), reviewed.clone())]);
        assert_eq!(world.state(), UpdateState::Staged);
        // The staged screen reflects what the system read back, not the reply.
        assert_eq!(
            route(&desk),
            Route::UpdateStaged,
            "{}",
            desk.rows().join("\n")
        );
        assert!(desk.shows("System reports: update staged"));
        assert!(desk.shows("Staged packages: wros, wros-help"));
        assert!(desk.shows("▸ Not now · leave the update staged"));
        assert!(
            world.count(|r| *r == Request::RecoveryStatus) >= 2,
            "status was not re-read after staging"
        );
        // Nothing restarted and the writing context is untouched.
        assert_eq!(world.count(|r| matches!(r, Request::Power(_))), 0);
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary).unwrap(), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary).unwrap(), secondary);
        assert!(desk.app.deck.shell.update_flow.review.is_none());
    }
}

/// Stand in for the backup helper: the next backup answers with `result`.
fn backup_answers(desk: &mut Desk, result: Result<bool, &str>) {
    let root = desk.app.project.root.to_string_lossy().into_owned();
    let mut status = crate::backup_service::Status {
        configured: true,
        ..Default::default()
    };
    let result = result.map(|fresh| {
        if fresh {
            status.snapshots = vec![crate::backup_service::Snapshot {
                id: "b".repeat(64),
                time: chrono::Utc::now().to_rfc3339(),
                _path: root,
                project: "Book".into(),
                restore_checked_at: String::new(),
                has_profile: false,
            }];
        }
        status
    });
    let result = result.map_err(str::to_string);
    desk.app.deck.shell.backup.job =
        Some(crate::backup_service::spawn_work("test-backup", move |_, _| result).unwrap());
}

/// Tick the app until the back-up-then-stage chain has nothing left to do.
fn wait_chain(desk: &mut Desk) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while (desk.app.deck.shell.update_flow.chain.is_some() || desk.app.deck.busy)
        && std::time::Instant::now() < end
    {
        desk.app.tick_deck();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        desk.app.deck.shell.update_flow.chain.is_none(),
        "{:?}: {}",
        desk.app.deck.shell.update_flow.chain,
        desk.app.deck.note
    );
    desk.draw();
}

#[test]
fn back_up_then_stage_stages_only_after_a_verified_backup_and_restarts_only_when_confirmed() {
    // A backup that fails stages nothing and says which step stopped.
    let mut desk = Desk::open(book("update-chain-fail", true), 100, 30);
    let world = UpdateWorld::install(&mut desk);
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    reach_confirm(&mut desk);
    set_backup(&mut desk, true, false);
    assert!(
        desk.shows("Back up, then stage"),
        "{}",
        desk.rows().join("\n")
    );
    assert!(desk.shows("Back up, stage and restart"));
    desk.choose("Back up, then stage");
    assert!(desk.app.deck.shell.update_flow.chain.is_some());
    backup_answers(&mut desk, Err("Google Drive could not be reached"));
    wait_chain(&mut desk);
    assert_eq!(world.mutations(), 0, "staged without a verified backup");
    assert!(
        desk.app
            .deck
            .note
            .contains("The backup did not finish, so nothing was staged"),
        "{}",
        desk.app.deck.note
    );
    assert!(
        desk.app
            .deck
            .note
            .contains("Google Drive could not be reached")
    );

    // A verified backup stages exactly the reviewed update, and nothing restarts.
    let reviewed = desk
        .app
        .deck
        .shell
        .update_flow
        .review
        .clone()
        .unwrap()
        .review_id;
    desk.choose("Back up, then stage");
    backup_answers(&mut desk, Ok(true));
    wait_chain(&mut desk);
    assert_eq!(world.stages(), [(MEDIA.to_string(), reviewed)]);
    assert_eq!(route(&desk), Route::UpdateStaged);
    assert_eq!(world.count(|r| matches!(r, Request::Power(_))), 0);

    // The restart variant restarts once the system reports the update staged.
    let mut desk = Desk::open(book("update-chain-restart", true), 100, 30);
    let world = UpdateWorld::install(&mut desk);
    desk.typed("Words saved before the restart. ");
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    reach_confirm(&mut desk);
    set_backup(&mut desk, true, false);
    desk.choose("Back up, stage and restart");
    backup_answers(&mut desk, Ok(true));
    wait_chain(&mut desk);
    assert_eq!(world.stages().len(), 1);
    assert_eq!(world.state(), UpdateState::Staged);
    assert_eq!(
        world.count(|r| *r == Request::Power(writingdeck_sys::PowerAction::Restart)),
        1,
        "{}",
        desk.app.deck.note
    );
    assert_eq!(desk.app.project.dirty_count(), 0, "words were saved first");

    // A backup that finished but left no fresh snapshot is not trusted either.
    let mut desk = Desk::open(book("update-chain-stale", true), 100, 30);
    let world = UpdateWorld::install(&mut desk);
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    reach_confirm(&mut desk);
    set_backup(&mut desk, true, false);
    desk.choose("Back up, stage and restart");
    backup_answers(&mut desk, Ok(false));
    wait_chain(&mut desk);
    assert_eq!(world.mutations(), 0);
}

#[test]
fn an_app_only_update_installs_now_and_restarts_only_the_writing_app() {
    use writingdeck_sys::PowerAction;
    let mut desk = Desk::open(book("update-live", true), 100, 30);
    let world = UpdateWorld::install(&mut desk);
    world.sim.lock().unwrap().staged_live = true;
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    reach_confirm(&mut desk);
    set_backup(&mut desk, true, true);
    desk.choose("Stage this update");
    wait_system(&mut desk);
    assert_eq!(route(&desk), Route::UpdateStaged);
    assert!(
        desk.shows("Install now · only the writing app restarts"),
        "{}",
        desk.rows().join("\n")
    );
    desk.choose("Install now");
    wait_system(&mut desk);
    assert_eq!(world.count(|r| *r == Request::ApplyStagedUpdate), 1);
    assert_eq!(world.count(|r| matches!(r, Request::Power(_))), 0);
    assert!(
        desk.app.deck.app_restart_requested,
        "{}",
        desk.app.deck.note
    );
    assert!(
        desk.app.deck.note.contains("restarting the writing app"),
        "{}",
        desk.app.deck.note
    );

    // An update that needs a restart never offers to install now.
    let mut desk = Desk::open(book("update-not-live", true), 100, 30);
    let _world = UpdateWorld::install(&mut desk);
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    reach_confirm(&mut desk);
    set_backup(&mut desk, true, true);
    desk.choose("Stage this update");
    wait_system(&mut desk);
    assert_eq!(route(&desk), Route::UpdateStaged);
    assert!(
        !desk.shows("Install now"),
        "{}
{:?}",
        desk.rows().join(
            "
"
        ),
        desk.app.deck.shell.update_flow.staged
    );
    assert!(!desk.app.deck.app_restart_requested);

    // One press with restart: an app-only update installs now instead of rebooting.
    let mut desk = Desk::open(book("update-live-chain", true), 100, 30);
    let world = UpdateWorld::install(&mut desk);
    world.sim.lock().unwrap().staged_live = true;
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    reach_confirm(&mut desk);
    set_backup(&mut desk, true, false);
    desk.choose("Back up, stage and restart");
    backup_answers(&mut desk, Ok(true));
    wait_chain(&mut desk);
    assert_eq!(world.count(|r| *r == Request::ApplyStagedUpdate), 1);
    assert_eq!(
        world.count(|r| *r == Request::Power(PowerAction::Restart)),
        0
    );
    assert!(
        desk.app.deck.app_restart_requested,
        "{}",
        desk.app.deck.note
    );
}

#[test]
fn update_backup_not_set_up_does_not_block_but_is_shown_and_no_backup_is_never_called_fresh() {
    let mut desk = Desk::open(book("update-no-backup", true), 100, 30);
    let world = UpdateWorld::install(&mut desk);
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    reach_confirm(&mut desk);
    set_backup(&mut desk, false, false);
    assert!(desk.shows("Backup: none is set up"));
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Enter);
    wait_system(&mut desk);
    assert_eq!(world.stages().len(), 1);
    assert_eq!(route(&desk), Route::UpdateStaged);
}

#[test]
fn a_changed_bundle_returns_to_review_with_an_explanation_and_stages_only_the_new_id() {
    for (width, height) in SIZES {
        let mut desk = Desk::open(
            book(&format!("update-changed-{width}"), true),
            width,
            height,
        );
        let world = UpdateWorld::install(&mut desk);
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        reach_confirm(&mut desk);
        set_backup(&mut desk, true, true);
        let old = desk.app.deck.shell.update_flow.review.clone().unwrap();
        // The media is swapped after the writer reviewed it.
        world.sim.lock().unwrap().bundle_revision += 1;
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(world.state(), UpdateState::Idle, "nothing may be staged");
        assert_eq!(
            route(&desk),
            Route::UpdateReview,
            "{}",
            desk.rows().join("\n")
        );
        assert!(desk.shows("Changed since your last review"));
        let shown = desk.app.deck.shell.update_flow.review.clone().unwrap();
        assert_ne!(shown.review_id, old.review_id);
        assert_eq!(shown.review_id, world.sim.lock().unwrap().review_id());
        assert!(desk.shows("▸ Back · do not install anything"));
        // Exactly one stage attempt, with the id the writer had approved.
        assert_eq!(world.stages(), [(MEDIA.to_string(), old.review_id.clone())]);

        // The writer approves the new list explicitly; only then is it staged.
        desk.choose("Continue to confirmation");
        wait_backup(&mut desk);
        set_backup(&mut desk, true, true);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(world.stages().len(), 2);
        assert_eq!(world.stages()[1].1, shown.review_id);
        assert_eq!(world.state(), UpdateState::Staged);
        assert_eq!(route(&desk), Route::UpdateStaged);
    }
}

#[test]
fn a_failed_stage_of_an_unchanged_update_says_so_and_stays_on_the_confirmation() {
    let mut desk = Desk::open(book("update-stage-fails", true), 100, 30);
    let world = UpdateWorld::install(&mut desk);
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    reach_confirm(&mut desk);
    set_backup(&mut desk, true, true);
    world.fail_stage.store(true, Ordering::Relaxed);
    desk.key(KeyCode::Down);
    desk.key(KeyCode::Enter);
    wait_system(&mut desk);
    assert_eq!(world.state(), UpdateState::Idle);
    assert_eq!(route(&desk), Route::UpdateConfirm);
    let result = desk.app.deck.shell.update_flow.last_result.clone().unwrap();
    assert!(
        result.contains("the helper could not write its journal"),
        "{result}"
    );
    assert!(result.contains("unchanged"), "{result}");
    assert!(result.contains("nothing was installed"), "{result}");
    assert!(desk.app.deck.shell.update_flow.recheck.is_none());
    // The Cancel row is still the way out, and retrying works once the helper does.
    world.fail_stage.store(false, Ordering::Relaxed);
    set_backup(&mut desk, true, true);
    desk.key(KeyCode::Enter);
    wait_system(&mut desk);
    assert_eq!(world.state(), UpdateState::Staged);
}

#[test]
fn staged_update_restart_needs_saved_work_and_cancel_rereads_what_the_system_says() {
    for (width, height) in SIZES {
        let (mut desk, note) =
            secondary_book(&format!("update-staged-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        let world = UpdateWorld::install(&mut desk);
        desk.typed("Words that must survive the restart request. ");
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        reach_confirm(&mut desk);
        set_backup(&mut desk, true, true);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(route(&desk), Route::UpdateStaged);

        // "Not now" is the default and sends nothing; the staged update stays.
        desk.key(KeyCode::Enter);
        assert_eq!(
            route(&desk),
            Route::Panel(crate::app::shell::Panel::Updates)
        );
        assert_eq!(world.state(), UpdateState::Staged);
        assert_eq!(world.mutations(), 1, "only the stage itself");
        assert!(desk.shows("An update is staged"));

        // Restart is refused while work is unsaved or conflicting.
        close_all(&mut desk);
        desk.typed("Unsaved edit. ");
        fs::write(&note, "External change while an update is staged.\n").unwrap();
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        desk.choose("An update is staged");
        assert_eq!(route(&desk), Route::UpdateStaged);
        desk.app.deck.note.clear();
        desk.choose("Restart now to install the staged update");
        assert_eq!(world.count(|r| matches!(r, Request::Power(_))), 0);
        assert!(
            desk.app
                .deck
                .note
                .contains("unsaved buffers or source conflicts"),
            "{}",
            desk.app.deck.note
        );
    }

    for (width, height) in SIZES {
        let (mut desk, note) =
            secondary_book(&format!("update-restart-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        let world = UpdateWorld::install(&mut desk);
        desk.typed("Saved words before restart. ");
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        reach_confirm(&mut desk);
        set_backup(&mut desk, true, true);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(route(&desk), Route::UpdateStaged);
        desk.choose("Restart now to install the staged update");
        wait_system(&mut desk);
        assert_eq!(
            world.count(|r| *r == Request::Power(writingdeck_sys::PowerAction::Restart)),
            1
        );
        assert_eq!(desk.app.project.dirty_count(), 0, "work was saved first");

        // A restart is only offered while the system confirms a staged update.
        world.sim.lock().unwrap().recovery.update_state = UpdateState::Idle;
        desk.app.refresh_recovery();
        wait_system(&mut desk);
        desk.choose("Restart now to install the staged update");
        assert_eq!(
            world.count(|r| matches!(r, Request::Power(_))),
            1,
            "restart requested without a confirmed staged update"
        );
    }
}

#[test]
fn cancelling_a_staged_update_is_never_assumed_and_follows_the_readback() {
    for (width, height) in SIZES {
        let mut desk = Desk::open(book(&format!("update-cancel-{width}"), true), width, height);
        let world = UpdateWorld::install(&mut desk);
        system_panel(&mut desk, 5);
        wait_system(&mut desk);
        reach_confirm(&mut desk);
        set_backup(&mut desk, true, true);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(route(&desk), Route::UpdateStaged);

        // The helper claims success but nothing changed: the screens must say so.
        world.cancel_is_noop.store(true, Ordering::Relaxed);
        desk.choose("Cancel staged update");
        wait_system(&mut desk);
        assert_eq!(
            desk.app
                .deck
                .shell
                .recovery
                .as_ref()
                .map(|r| r.update_state),
            Some(UpdateState::Staged),
            "cancel optimistically cleared the cached status"
        );
        assert_eq!(
            route(&desk),
            Route::Panel(crate::app::shell::Panel::Updates)
        );
        assert!(desk.shows("An update is staged"));
        assert!(desk.shows("Update state: staged"));

        // A real cancel returns the system to idle and the screens follow.
        world.cancel_is_noop.store(false, Ordering::Relaxed);
        desk.choose("An update is staged");
        desk.choose("Cancel staged update");
        wait_system(&mut desk);
        assert_eq!(world.state(), UpdateState::Idle);
        assert_eq!(
            desk.app
                .deck
                .shell
                .recovery
                .as_ref()
                .map(|r| r.update_state),
            Some(UpdateState::Idle)
        );
        assert!(!desk.shows("An update is staged"));
        assert!(desk.shows("Last update result: SIMULATED: staged update cancelled"));
        assert!(desk.app.deck.shell.update_flow.staged.is_none());
    }
}

#[test]
fn recovery_shows_update_state_and_repair_sits_behind_a_default_cancel_confirmation() {
    for (width, height) in SIZES {
        let (mut desk, note) =
            secondary_book(&format!("update-repair-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        let world = UpdateWorld::install(&mut desk);
        {
            let mut sim = world.sim.lock().unwrap();
            sim.recovery.interrupted_dpkg = true;
            sim.recovery.unconfigured_packages = vec!["grimoire".into()];
            sim.recovery.update_state = UpdateState::Interrupted;
        }
        desk.typed("Writing kept through repair. ");
        let original = desk.app.editor.text();
        system_panel(&mut desk, 6);
        wait_system(&mut desk);
        desk.choose("Recovery status, repair and staged update");
        wait_system(&mut desk);
        assert_eq!(route(&desk), Route::RecoveryView);
        assert!(desk.shows("Update state: interrupted"));
        assert!(desk.shows("Interrupted dpkg packages: grimoire"));
        assert!(desk.shows("ext4"));
        assert!(!desk.shows("not qualified"));

        // Repair: confirmation first, Cancel by default, nothing sent on Enter.
        desk.choose("Repair interrupted update or packages");
        assert_eq!(route(&desk), Route::RepairConfirm);
        assert!(desk.shows("▸ Cancel · do not repair"));
        desk.key(KeyCode::Enter);
        assert_eq!(route(&desk), Route::RecoveryView);
        assert_eq!(world.mutations(), 0);

        // A repair the helper only claims is shown as still needed.
        world.repair_is_noop.store(true, Ordering::Relaxed);
        desk.choose("Repair interrupted update or packages");
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(world.count(|r| *r == Request::RecoverUpdates), 1);
        assert_eq!(route(&desk), Route::RecoveryView);
        assert!(
            desk.shows("Interrupted dpkg packages: grimoire"),
            "repair optimistically cleared the cached status:\n{}",
            desk.rows().join("\n")
        );

        // A real repair is followed by a fresh read of the system.
        world.repair_is_noop.store(false, Ordering::Relaxed);
        desk.choose("Repair interrupted update or packages");
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(world.count(|r| *r == Request::RecoverUpdates), 2);
        assert!(!desk.shows("Interrupted dpkg packages"));
        assert!(desk.shows("Update state: recovered"));
        assert!(desk.shows("Last result: SIMULATED: repair finished"));
        for _ in 0..4 {
            desk.key(KeyCode::Esc);
        }
        assert_eq!(desk.app.overlay, Overlay::None);
        assert_eq!(desk.app.editor.text(), original);
    }
}

#[test]
fn recovery_during_apply_offers_no_cancel_or_repair() {
    let mut desk = Desk::open(book("update-applying", true), 100, 30);
    let world = UpdateWorld::install(&mut desk);
    world.sim.lock().unwrap().recovery.update_state = UpdateState::Applying;
    system_panel(&mut desk, 6);
    wait_system(&mut desk);
    desk.choose("Recovery status, repair and staged update");
    wait_system(&mut desk);
    assert!(desk.shows("Update state: applying"));
    assert!(desk.shows("cannot be cancelled"));
    assert!(!desk.shows("Cancel staged update"));
    assert!(!desk.shows("Repair"));
    // Even walking every row sends no mutating request.
    for _ in 0..12 {
        desk.key(KeyCode::Down);
    }
    assert_eq!(world.mutations(), 0);
}

#[test]
fn google_token_file_form_fills_in_the_mounted_stick_and_never_reviews_it_as_an_update() {
    use crate::app::backup::FormKind;
    let mut desk = Desk::open(book("google-token-file", true), 100, 30);
    let world = UpdateWorld::install(&mut desk);
    desk.app.open_settings();
    desk.app.deck.shell.nav.open(Route::Backups);
    desk.app
        .open_backup_form(FormKind::GoogleFile, String::new());
    wait_system(&mut desk);
    let rows = desk.app.deck.shell.backup.form.as_ref().unwrap().rows();
    assert_eq!(
        rows[0],
        "Authorization file (absolute path): /run/wros-update/media/0/wros-drive-token.json"
    );
    assert!(
        desk.app
            .deck
            .shell
            .backup
            .notice
            .starts_with("Stick mounted read-only at /run/wros-update/media/0.")
    );
    assert_eq!(route(&desk), Route::BackupForm);
    assert!(world.calls().contains(&Request::FindUpdateMedia));
    assert_eq!(
        world.count(|r| matches!(r, Request::ReviewUpdateBundle { .. })),
        0
    );

    // Without a stick the path stays empty for typing, and Escape still discards the form.
    desk.key(KeyCode::Esc);
    world.sim.lock().unwrap().media = None;
    desk.app
        .open_backup_form(FormKind::GoogleFile, String::new());
    wait_system(&mut desk);
    assert!(
        desk.app
            .deck
            .shell
            .backup
            .notice
            .starts_with("No stick labelled WROS-UPDATE")
    );
    desk.app.paste("/home/writer/token.json");
    let rows = desk.app.deck.shell.backup.form.as_ref().unwrap().rows();
    assert_eq!(
        rows[0],
        "Authorization file (absolute path): /home/writer/token.json"
    );
    desk.key(KeyCode::Esc);
    assert!(desk.app.deck.shell.backup.form.is_none());
    assert_eq!(route(&desk), Route::Backups);
}

#[test]
fn update_failures_say_what_the_writer_can_do_next() {
    let mut desk = Desk::open(book("update-errors", true), 100, 30);
    let world = UpdateWorld::install(&mut desk);
    system_panel(&mut desk, 5);
    wait_system(&mut desk);

    // No media inserted.
    world.sim.lock().unwrap().media = None;
    desk.choose("Find update media");
    wait_system(&mut desk);
    assert_eq!(
        route(&desk),
        Route::Panel(crate::app::shell::Panel::Updates)
    );
    assert!(desk.app.deck.note.contains("No update media found"));
    assert!(desk.app.deck.note.contains("WROS-UPDATE"));
    assert!(desk.shows("Last update result: No update media found"));

    // A bundle the helper cannot authenticate.
    struct Failing(Arc<Mutex<Vec<Request>>>);
    impl Backend for Failing {
        fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
            self.0.lock().unwrap().push(request.clone());
            match request {
                Request::ReviewUpdateBundle { .. } => {
                    anyhow::bail!("the signature does not match a pinned WrOS key")
                }
                Request::FindUpdateMedia => anyhow::bail!("pkexec: Not authorized"),
                other => Simulated::laptop().request(other),
            }
        }
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    desk.app
        .deck
        .use_test_backend(Box::new(Failing(seen.clone())));
    review_by_path(&mut desk, MEDIA);
    assert_eq!(
        route(&desk),
        Route::UpdateBundleForm,
        "stays to correct the path"
    );
    let note = desk.app.deck.note.clone();
    assert!(note.contains("Update not reviewed"), "{note}");
    assert!(note.contains("signature does not match"), "{note}");
    assert!(note.contains("Nothing was changed"), "{note}");
    assert!(note.contains("try again"), "{note}");
    desk.key(KeyCode::Esc);
    desk.key(KeyCode::Home);
    desk.choose("Find update media");
    wait_system(&mut desk);
    let note = desk.app.deck.note.clone();
    assert!(note.contains("Not authorized"), "{note}");
    assert!(note.contains("Nothing was mounted"), "{note}");
    assert!(note.contains("enter a folder path"), "{note}");
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .all(|r| !matches!(r, Request::StageUpdates { .. } | Request::Power(_)))
    );
}

#[test]
fn updates_and_recovery_without_a_service_do_not_claim_anything() {
    let mut desk = Desk::open(book("update-no-service", true), 80, 24);
    // No backend at all (as in a test session): opening the panels is harmless.
    system_panel(&mut desk, 5);
    assert!(!desk.app.deck.busy);
    assert!(desk.app.deck.shell.recovery.is_none());
    assert!(!desk.shows("An update is staged"));
    desk.choose("Review a signed update bundle by path");
    desk.app.paste(MEDIA);
    desk.key(KeyCode::Enter);
    desk.key(KeyCode::Enter);
    assert!(!desk.app.deck.busy);
    assert_eq!(route(&desk), Route::UpdateBundleForm);
    assert!(desk.app.deck.shell.update_flow.review.is_none());
}

#[test]
fn live_installer_handoff_confirmation_and_dirty_buffer_gate_preserve_both_editors_at_four_sizes() {
    use writingdeck_sys::Simulated;
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("live-installer-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.app
            .deck
            .use_test_backend(Box::new(Simulated::laptop()));
        desk.typed("Live session draft before starting encrypted installer. ");
        let original = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary);
        let secondary = desk.app.view_position(Pane::Secondary);

        // Installed session (live_session == false) refuses live installer handoff.
        desk.app.deck.live_session = false;
        desk.app.deck.installer_available = true;
        desk.app.request_live_installer();
        assert!(!desk.app.deck.installer_requested);

        // Enable live_session and open System > Storage (index 3).
        desk.app.deck.live_session = true;
        desk.app.deck.installer_available = true;
        system_panel(&mut desk, 3);
        wait_system(&mut desk);
        desk.choose("Install WrOS to disk · encrypted live installer");
        assert_eq!(desk.app.deck.shell.nav.current(), Route::InstallerConfirm);
        assert!(desk.shows("Cancel and remain in the live writing session"));

        // Default selection is Cancel (row 0).
        desk.key(KeyCode::Enter);
        assert_eq!(
            desk.app.deck.shell.nav.current(),
            Route::Panel(crate::app::shell::Panel::Storage)
        );
        assert!(!desk.app.deck.installer_requested);

        // Open confirmation again and choose Start encrypted WrOS installer (row 1).
        desk.choose("Install WrOS to disk · encrypted live installer");
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        assert!(desk.app.deck.installer_requested);
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary), secondary);

        // Reset flag and verify source conflict blocks installer handoff.
        desk.app.deck.installer_requested = false;
        for _ in 0..3 {
            desk.key(KeyCode::Esc);
        }
        assert_eq!(desk.app.overlay, Overlay::None);
        desk.typed("Unsaved live edit with external conflict. ");
        fs::write(&note, "External file modification on live storage.\n").unwrap();
        desk.app.request_live_installer();
        assert!(!desk.app.deck.installer_requested);
        assert!(
            desk.app
                .deck
                .shell
                .notices
                .latest()
                .unwrap()
                .contains("unsaved buffers or source conflicts")
        );
    }
}

#[test]
fn manual_clock_form_default_cancel_and_verified_apply_preserve_both_editors_at_four_sizes() {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use writingdeck_sys::{Backend, Reply, Request, Simulated};
    struct ClockBackend {
        calls: Arc<Mutex<Vec<Request>>>,
        fail: Arc<AtomicBool>,
        simulator: Simulated,
    }
    impl Backend for ClockBackend {
        fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
            self.calls.lock().unwrap().push(request.clone());
            if matches!(request, Request::SetLocalClock(_)) && self.fail.load(Ordering::Relaxed) {
                anyhow::bail!("clock authorization unavailable");
            }
            self.simulator.request(request)
        }
    }
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("manual-clock-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        let calls = Arc::new(Mutex::new(vec![]));
        let fail = Arc::new(AtomicBool::new(false));
        desk.app.deck.use_test_backend(Box::new(ClockBackend {
            calls: calls.clone(),
            fail: fail.clone(),
            simulator: Simulated::laptop(),
        }));
        desk.typed("Original manual clock draft. ");
        let original = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary);
        let secondary = desk.app.view_position(Pane::Secondary);

        system_panel(&mut desk, 4);
        wait_system(&mut desk);
        desk.choose("Set the clock by hand (turns network time off)");
        assert_eq!(desk.app.deck.shell.nav.current(), Route::ManualClock);

        // Invalid datetime is rejected before opening confirmation.
        desk.app.paste("2026-13-40 99:00");
        desk.key(KeyCode::Enter);
        desk.key(KeyCode::Enter);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::ManualClock);
        assert!(desk.shows("Unavailable:"));

        // Valid datetime advances to default-Cancel confirmation.
        desk.key(KeyCode::Home);
        desk.key(KeyCode::Delete);
        desk.app.paste("2026-11-01 09:30");
        desk.key(KeyCode::Enter);
        desk.key(KeyCode::Enter);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::ManualClockConfirm);
        assert!(desk.shows("Cancel manual clock change"));
        desk.key(KeyCode::Enter); // Default Cancel sends no SetLocalClock request.
        assert_eq!(desk.app.deck.shell.nav.current(), Route::ManualClock);
        assert!(
            !calls
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, Request::SetLocalClock(_)))
        );

        // Confirm with simulated failure keeps the confirmation route open.
        desk.key(KeyCode::Enter);
        assert_eq!(desk.app.deck.shell.nav.current(), Route::ManualClockConfirm);
        desk.key(KeyCode::Down);
        fail.store(true, Ordering::Relaxed);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert!(desk.shows("clock authorization unavailable"));
        assert_eq!(desk.app.deck.shell.nav.current(), Route::ManualClockConfirm);

        // Confirm with working service applies and returns to DateTime panel.
        fail.store(false, Ordering::Relaxed);
        desk.key(KeyCode::Enter);
        wait_system(&mut desk);
        assert_eq!(
            desk.app.deck.shell.nav.current(),
            Route::Panel(crate::app::shell::Panel::DateTime)
        );
        assert!(desk.shows("Clock set to 2026-11-01 09:30:00; network time is off"));
        // Network time can be switched back on from the same panel.
        wait_system(&mut desk);
        assert!(desk.shows("Network time: off"));
        desk.choose("Use network time again (sets the clock automatically)");
        wait_system(&mut desk);
        assert!(
            calls
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, Request::UseNetworkTime))
        );
        assert!(desk.shows("Network time is on"));
        wait_system(&mut desk);
        assert!(desk.shows("Network time: on"));
        assert!(!desk.shows("Use network time again"));
        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary), secondary);
    }
}

#[test]
fn sound_and_volume_panel_controls_and_keyboard_wizard_alias_registration_preserve_both_editors_at_four_sizes()
 {
    use writingdeck_sys::Simulated;
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("sound-kbd-wizard-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.app
            .deck
            .use_test_backend(Box::new(Simulated::laptop()));
        desk.typed("Draft preserved across sound and keyboard wizard. ");
        let original = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary);
        let secondary = desk.app.view_position(Pane::Secondary);

        // Interface > First-run keyboard layout wizard.
        desk.app.open_settings();
        desk.draw();
        desk.key(KeyCode::Home);
        desk.key(KeyCode::Down);
        desk.key(KeyCode::Enter);
        assert_eq!(
            desk.app.deck.shell.nav.current(),
            Route::Category(Category::Interface)
        );
        desk.choose("First-run keyboard layout wizard · registered maps only");
        assert_eq!(desk.app.deck.shell.nav.current(), Route::KeyboardWizard);
        desk.key(KeyCode::Enter); // Cycle base layout from QWERTY to Colemak.
        assert!(desk.shows("Colemak (home-row ARST / NEIO)"));
        desk.key(KeyCode::Down); // Select alias field.
        desk.app.paste("writer-colemak");
        desk.key(KeyCode::Enter); // Advances to Register local alias row.
        desk.key(KeyCode::Enter); // Registers alias.
        assert_eq!(desk.app.deck.shell.custom_keymaps.len(), 1);
        assert_eq!(desk.app.deck.shell.custom_keymaps[0].name, "writer-colemak");
        desk.key(KeyCode::Esc);
        assert_eq!(
            desk.app.deck.shell.nav.current(),
            Route::Category(Category::Interface)
        );

        if cfg!(feature = "audio") {
            desk.key(KeyCode::Esc);
            desk.key(KeyCode::Down);
            desk.key(KeyCode::Enter);
            assert_eq!(
                desk.app.deck.shell.nav.current(),
                Route::Category(Category::System)
            );
            desk.choose("Sound and volume");
            wait_system(&mut desk);
            assert_eq!(desk.app.deck.shell.nav.current(), Route::SoundView);
            assert_eq!(
                desk.app
                    .deck
                    .shell
                    .audio_status
                    .as_ref()
                    .and_then(|a| a.volume_percent),
                Some(70)
            );
            desk.choose("Volume +10%");
            wait_system(&mut desk);
            assert_eq!(
                desk.app
                    .deck
                    .shell
                    .audio_status
                    .as_ref()
                    .and_then(|a| a.volume_percent),
                Some(80)
            );
            desk.choose("Mute sound output");
            wait_system(&mut desk);
            assert_eq!(
                desk.app.deck.shell.audio_status.as_ref().map(|a| a.muted),
                Some(true)
            );
        }

        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary), secondary);
    }
}

#[test]
fn pre_sleep_lock_protocol_verification_enforces_save_gate_and_keeps_hardware_suspend_disabled_at_four_sizes()
 {
    use writingdeck_sys::Simulated;
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut desk, note) =
            secondary_book(&format!("pre-sleep-lock-{width}"), width, height, true);
        choose_secondary(&mut desk, &note);
        desk.app
            .deck
            .use_test_backend(Box::new(Simulated::laptop()));
        desk.typed("Draft preserved across pre-sleep lock verification. ");
        let original = desk.app.editor.text();
        let primary = desk.app.view_position(Pane::Primary);
        let secondary = desk.app.view_position(Pane::Secondary);

        desk.app.deck.lock_available = true;
        system_panel(&mut desk, 2);
        wait_system(&mut desk);
        // A readiness check, not a writer's choice: sleep stays off until lock-before-sleep ships.
        assert!(!desk.shows("Verify pre-sleep lock readiness"));
        let check = desk.app.verify_pre_sleep_lock_protocol().unwrap();
        assert!(check.buffers_saved && check.input_retired_ready && check.lock_handoff_armed);
        assert!(!check.suspend_permitted);
        assert!(check.message.contains("Suspend unavailable"));

        assert_eq!(desk.app.editor.text(), original);
        assert_eq!(desk.app.view_position(Pane::Primary), primary);
        assert_eq!(desk.app.view_position(Pane::Secondary), secondary);
    }
}

#[test]
fn going_back_is_offered_only_with_a_kept_version_and_asks_before_staging() {
    let mut desk = Desk::open(book("update-go-back", true), 120, 30);
    let world = UpdateWorld::install(&mut desk);
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    desk.app.refresh_recovery();
    wait_system(&mut desk);
    assert!(!desk.shows("Go back to"), "{}", desk.rows().join("\n"));

    world.sim.lock().unwrap().recovery.previous_version = Some("wros 0.1.0~rc4".into());
    desk.app.refresh_recovery();
    wait_system(&mut desk);
    desk.key(KeyCode::Home);
    desk.choose("Go back to wros 0.1.0~rc4");
    assert_eq!(route(&desk), Route::GoBackConfirm);
    assert_eq!(
        world.count(|r| *r == Request::GoBackToPreviousVersion),
        0,
        "nothing is staged before the writer confirms"
    );
    assert!(
        desk.shows("Stay on this version"),
        "{}",
        desk.rows().join("\n")
    );
    desk.choose("Go back to wros 0.1.0~rc4 · installs at the next restart");
    wait_system(&mut desk);
    assert_eq!(world.count(|r| *r == Request::GoBackToPreviousVersion), 1);
    assert_eq!(route(&desk), Route::UpdateStaged);
}

#[test]
fn a_new_version_on_probation_reports_ready_once_after_the_first_screen() {
    for probation in [true, false] {
        let mut desk = Desk::open(book(&format!("update-ready-{probation}"), true), 120, 30);
        let world = UpdateWorld::install(&mut desk);
        world.sim.lock().unwrap().recovery.probation = probation;
        desk.app.deck.online_updates = true;
        desk.app.tick_deck();
        assert!(
            !desk.app.deck.busy,
            "nothing is read the moment the session starts"
        );
        desk.app.deck.shell.update_flow.health_at = Some(std::time::Instant::now());
        for _ in 0..3 {
            desk.app.tick_deck();
            wait_system(&mut desk);
        }
        assert_eq!(
            world.count(|r| *r == Request::ReportUpdateReady),
            usize::from(probation)
        );
    }
}

#[test]
fn after_going_back_by_itself_wros_says_so_after_login() {
    let mut desk = Desk::open(book("update-went-back", true), 120, 30);
    let world = UpdateWorld::install(&mut desk);
    {
        let mut sim = world.sim.lock().unwrap();
        sim.recovery.went_back = true;
        sim.recovery.went_back_automatically = true;
    }
    desk.app.deck.online_updates = true;
    desk.app.deck.shell.update_flow.health_at = Some(std::time::Instant::now());
    desk.app.tick_deck();
    wait_system(&mut desk);
    assert!(
        desk.app
            .deck
            .note
            .contains("went back to the previous version"),
        "{}",
        desk.app.deck.note
    );
    system_panel(&mut desk, 5);
    wait_system(&mut desk);
    assert!(
        desk.shows("WrOS went back to this version"),
        "{}",
        desk.rows().join("\n")
    );
}
