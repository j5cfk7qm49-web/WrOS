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
    assert!(desk.shows("On battery, WrOS warns at 20% and 10%."));
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
    use crate::battery::{GRACE, Reading};
    use std::time::Instant;
    let plugged = Reading::default();
    let reserve = Reading {
        percent: Some(8),
        discharging: true,
        critical: true,
    };
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
        let now = Instant::now();
        assert_eq!(desk.app.power_event(true, plugged, now), lid, "{choice:?}");
        assert_eq!(
            desk.app.power_event(false, plugged, now),
            None,
            "{choice:?}"
        );
        // The battery counts down first, then sleeps, whatever the lid choice.
        assert_eq!(
            desk.app.power_event(false, reserve, now),
            None,
            "{choice:?}"
        );
        assert_eq!(
            desk.app.power_event(false, reserve, now + GRACE),
            Some(true),
            "{choice:?}"
        );
        desk.app.deck.battery = Default::default();
    }
    // Where sleep is unavailable, lock and sleep only locks, and the battery only warns.
    desk.app.deck.suspend_available = false;
    desk.app.deck.lid_close = LidClose::LockAndSleep;
    let now = Instant::now();
    assert_eq!(desk.app.power_event(true, plugged, now), Some(false));
    assert_eq!(desk.app.power_event(false, reserve, now), None);
    assert_eq!(desk.app.power_event(false, reserve, now + GRACE), None);
    // Without the lock, a closed lid asks for nothing at all.
    desk.app.deck.lock_available = false;
    assert_eq!(desk.app.power_event(true, plugged, now), None);
}

#[test]
fn battery_warnings_and_the_countdown_show_in_the_status_bar_and_notices() {
    use crate::battery::{GRACE, Reading};
    use std::time::Instant;
    let on = |percent: u8| Reading {
        percent: Some(percent),
        discharging: true,
        critical: percent <= 8,
    };
    let mut desk = Desk::open(book("battery-warnings", true), 120, 30);
    desk.app.deck.lock_available = true;
    desk.app.deck.suspend_available = true;
    let now = Instant::now();
    desk.app.power_event(false, on(20), now);
    desk.draw();
    assert!(
        desk.status()
            .contains("Battery 20%. Plug in soon. At 8% WrOS saves"),
        "{}",
        desk.status()
    );
    desk.app.power_event(false, on(10), now);
    desk.draw();
    assert!(desk.status().contains("Battery 10%. Plug in now."));
    // At the reserve: saved first, then a countdown, and the notices keep it once.
    desk.app.focus = Focus::Editor;
    desk.typed("Words before the battery runs out. ");
    assert!(desk.app.project.dirty_count() > 0);
    assert_eq!(desk.app.power_event(false, on(8), now), None);
    assert_eq!(
        desk.app.project.dirty_count(),
        0,
        "saved when the countdown starts"
    );
    desk.draw();
    assert!(
        desk.status()
            .contains("sleeps in 30 seconds. Plug in to stay awake.")
    );
    desk.app.power_event(false, on(8), now + GRACE / 2);
    let notices: Vec<&str> = desk.app.deck.shell.notices.iter().collect();
    assert_eq!(
        notices.iter().filter(|n| n.contains("seconds")).count(),
        1,
        "{notices:?}"
    );
    assert_eq!(desk.app.power_event(false, on(8), now + GRACE), Some(true));
    // After unlocking, the status bar says why WrOS slept.
    desk.app.on_unlocked();
    desk.draw();
    assert!(desk.app.msg.contains("battery reached its reserve"));
}
