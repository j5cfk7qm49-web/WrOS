use super::*;
use crate::app::shell::{Category, Panel, Route};
use crate::lid::LidClose;

fn power_page(tag: &str) -> Desk {
    let mut desk = Desk::open(book(tag, true), 100, 30);
    desk.app.deck.simulate();
    desk.app.open_settings_at(&[
        Route::Category(Category::System),
        Route::Panel(Panel::Power),
    ]);
    desk.draw();
    desk
}

#[test]
fn the_lid_choice_shows_only_where_the_session_can_lock() {
    let mut desk = power_page("lid-hidden");
    desk.app.deck.lid_settings = Some(desk.root.join("home/.config/grimoire/lid.json"));
    desk.draw();
    assert!(!desk.shows("When the lid closes"));
}

#[test]
fn the_lid_choice_cycles_and_is_saved() {
    let mut desk = power_page("lid-cycle");
    let settings = desk.root.join("home/.config/grimoire/lid.json");
    desk.app.deck.lid_settings = Some(settings.clone());
    desk.app.deck.lock_available = true;
    desk.app.deck.suspend_available = true;
    desk.draw();
    assert!(desk.shows("When the lid closes: lock and sleep"));
    assert!(desk.shows("A nearly empty battery always saves, locks and sleeps."));
    // Lock writing session stays the last row: the VM lock test reaches it with End.
    let rows = desk.rows();
    let row = |label: &str| rows.iter().position(|r| r.contains(label)).unwrap();
    assert!(row("When the lid closes") < row("Shutdown"));
    assert!(row("Restart") < row("Lock writing session"));

    desk.choose("When the lid closes: lock and sleep");
    assert_eq!(crate::lid::load(&settings), LidClose::Lock);
    assert_eq!(
        desk.app.deck.shell.notices.latest(),
        Some("Closing the lid now saves and locks. The laptop stays awake.")
    );
    desk.choose("When the lid closes: lock, stay awake");
    assert_eq!(crate::lid::load(&settings), LidClose::Nothing);
    assert!(desk.shows("When the lid closes: do nothing"));
    desk.choose("When the lid closes: do nothing");
    assert_eq!(crate::lid::load(&settings), LidClose::LockAndSleep);
}

#[test]
fn a_closed_lid_does_what_the_writer_chose_and_a_low_battery_always_sleeps() {
    let mut desk = power_page("lid-events");
    let deck = &mut desk.app.deck;
    deck.lock_available = true;
    deck.suspend_available = true;
    for (choice, lid) in [
        (LidClose::LockAndSleep, Some(true)),
        (LidClose::Lock, Some(false)),
        (LidClose::Nothing, None),
    ] {
        desk.app.deck.lid_close = choice;
        assert_eq!(desk.app.power_event(true, false), lid, "{choice:?}");
        assert_eq!(desk.app.power_event(false, false), None, "{choice:?}");
        assert_eq!(desk.app.power_event(false, true), Some(true), "{choice:?}");
    }
    // Where sleep is unavailable, lock and sleep only locks, and the battery asks nothing.
    desk.app.deck.suspend_available = false;
    desk.app.deck.lid_close = LidClose::LockAndSleep;
    assert_eq!(desk.app.power_event(true, false), Some(false));
    assert_eq!(desk.app.power_event(false, true), None);
    // Without the lock, a closed lid asks for nothing at all.
    desk.app.deck.lock_available = false;
    assert_eq!(desk.app.power_event(true, false), None);
}
