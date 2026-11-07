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
    assert!(desk.shows("Graphical session (preview): off · from the next login"));

    desk.choose("Graphical session (preview): off");
    assert!(crate::session::graphical_chosen(&settings));
    assert_eq!(
        desk.app.deck.shell.notices.latest(),
        Some("Graphical session on. It starts at your next login.")
    );
    assert!(desk.shows("Graphical session (preview): on"));

    desk.choose("Graphical session (preview): on");
    assert!(!crate::session::graphical_chosen(&settings));
    assert!(desk.shows("Graphical session (preview): off"));
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
