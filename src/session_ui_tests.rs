use super::*;
use crate::app::shell::{Category, Panel, Route};

fn open_panel(desk: &mut Desk, panel: Panel) {
    desk.app
        .open_settings_at(&[Route::Category(Category::System), Route::Panel(panel)]);
    desk.draw();
}

fn display_page(tag: &str) -> Desk {
    let mut desk = Desk::open(book(tag, true), 100, 30);
    desk.app.deck.simulate();
    open_panel(&mut desk, Panel::Display);
    desk
}

#[test]
fn the_graphical_session_is_offered_only_when_it_can_start() {
    let desk = display_page("session-not-offered");
    assert!(!desk.shows("Graphical session"));
}

#[test]
fn turning_the_graphical_session_on_and_off_saves_the_choice() {
    let mut desk = display_page("session-toggle");
    let settings = desk.root.join("home/.config/wros/session.json");
    desk.app.deck.session_settings = Some(settings.clone());
    desk.draw();
    assert!(desk.shows("Graphical session: off · from the next login"));

    desk.choose("Graphical session: off");
    assert!(crate::session::graphical_chosen(&settings));
    assert_eq!(
        desk.app.deck.shell.notices.latest(),
        Some("Graphical session on. It starts at your next login.")
    );
    assert!(desk.shows("Graphical session: on"));

    desk.choose("Graphical session: on");
    assert!(!crate::session::graphical_chosen(&settings));
    assert!(desk.shows("Graphical session: off"));
}

#[test]
fn console_only_features_say_so_in_the_graphical_session() {
    let mut desk = display_page("session-graphical");
    desk.app.deck.graphical_session = true;
    desk.draw();
    assert!(desk.shows("Warm screen · not in the graphical session yet"));
    assert!(!desk.shows("Warm screen · blue light filter"));
    open_panel(&mut desk, Panel::Power);
    assert!(desk.shows("Lock and sleep · not in the graphical session yet"));
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
fn the_start_screen_choice_is_read_from_the_helper_and_can_be_changed_both_ways() {
    let mut desk = display_page("start-screen");
    wait_system(&mut desk);
    assert!(!desk.shows("Start screen:"));
    desk.app.deck.boot_settings_available = true;
    open_panel(&mut desk, Panel::Display);
    wait_system(&mut desk);
    assert!(desk.shows("Start screen: plain text"));
    desk.choose("Start screen: plain text");
    wait_system(&mut desk);
    assert!(desk.shows("Start screen: WrOS"));
    assert!(desk.shows("Preparing the start screen"));
    desk.choose("Start screen: WrOS");
    wait_system(&mut desk);
    assert!(desk.shows("Start screen: plain text"));
}

#[test]
fn accepted_themes_reach_the_start_screen_after_another_system_request_finishes() {
    use std::sync::{Arc, Mutex};
    use wros_sys::{Backend, Reply, Request, Simulated};
    struct Capture(Arc<Mutex<Vec<Request>>>, Simulated);
    impl Backend for Capture {
        fn request(&mut self, request: Request) -> anyhow::Result<Reply> {
            self.0.lock().unwrap().push(request.clone());
            self.1.request(request)
        }
    }
    let mut desk = display_page("start-screen-theme");
    wait_system(&mut desk);
    let calls = Arc::new(Mutex::new(Vec::new()));
    desk.app
        .deck
        .use_test_backend(Box::new(Capture(calls.clone(), Simulated::laptop())));
    desk.app.deck.boot_settings_available = true;
    desk.app.deck.request(Request::Refresh);
    desk.app
        .run_action(crate::palette::Action::Theme("Nord".into()));
    desk.app
        .run_action(crate::palette::Action::Theme("Dracula".into()));
    wait_system(&mut desk);
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|r| matches!(r, Request::BootTheme(_)))
            .count(),
        1
    );
    assert!(calls.contains(&Request::BootTheme(crate::theme::to_config(
        &desk.app.theme
    ))));
    assert_eq!(desk.app.theme.name, "Dracula");
}

#[test]
fn a_failed_start_screen_change_keeps_the_last_confirmed_choice() {
    let mut desk = display_page("start-screen-refused");
    wait_system(&mut desk);
    desk.app.deck.boot_settings_available = true;
    open_panel(&mut desk, Panel::Display);
    wait_system(&mut desk);
    let mut backend = wros_sys::Simulated::laptop();
    backend.fail_next = true;
    desk.app.deck.use_test_backend(Box::new(backend));
    desk.choose("Start screen: plain text");
    wait_system(&mut desk);
    assert!(desk.shows("Start screen: plain text"));
    assert!(
        desk.app
            .deck
            .note
            .contains("Couldn't change the start screen")
    );
}

#[test]
fn a_failed_boot_image_can_be_prepared_again_without_changing_the_choice() {
    let mut desk = display_page("start-screen-retry");
    wait_system(&mut desk);
    desk.app.deck.boot_settings_available = true;
    desk.app.deck.boot_status = Some(wros_sys::boot::Status {
        enabled: true,
        failed: true,
        ..Default::default()
    });
    desk.draw();
    desk.choose("Prepare the start screen again");
    wait_system(&mut desk);
    assert!(desk.shows("Start screen: WrOS"));
    assert!(desk.shows("Preparing the start screen"));
}

#[test]
fn an_existing_theme_is_carried_to_the_start_screen_once_after_an_update() {
    let mut desk = display_page("start-screen-existing-theme");
    wait_system(&mut desk);
    desk.app.deck.boot_settings_available = true;
    let path = desk.root.join("boot-theme.toml");
    desk.app.sync_boot_theme_if_changed(&path);
    desk.app.tick_deck();
    wait_system(&mut desk);
    assert!(desk.app.deck.boot_status.as_ref().unwrap().pending);
    fs::write(&path, crate::theme::to_config(&desk.app.theme)).unwrap();
    desk.app.sync_boot_theme_if_changed(&path);
    desk.app.tick_deck();
    assert!(
        !desk.app.deck.busy,
        "the same theme does not queue another rebuild"
    );
}
