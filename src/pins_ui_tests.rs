//! The pins sidebar behaves like the other side panes (rc13, section 8).
use super::*;

/// Two pinned documents, the sidebar shown, focus on it.
fn pinned(tag: &str) -> (Desk, PathBuf) {
    let (mut d, note) = secondary_book(tag, 160, 50, true);
    pin_document(&mut d, &note);
    let scene = first_scene(&d.root);
    pin_document(&mut d, &scene);
    d.key(KeyCode::Esc); // close the Pinboard
    if !d.app.pins.state().sidebar_visible {
        d.app.toggle_pins();
    }
    d.draw();
    let rect = d.app.pins.hits[0].1;
    d.app.on_click(rect.x + 1, rect.y + 1);
    d.draw();
    assert_eq!(d.app.focus, Focus::Pins);
    let file = grimoire_core::pins::Store::path(&fs::canonicalize(&d.root).unwrap());
    (d, file)
}

#[test]
fn moving_through_the_pins_sidebar_writes_nothing() {
    let (mut d, file) = pinned("pins-quiet");
    d.app.tick_pins();
    let before = fs::read(&file).unwrap();
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    for key in [
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::PageDown,
        KeyCode::PageUp,
    ] {
        for _ in 0..4 {
            d.key(key);
            d.app.tick_pins();
        }
    }
    let rect = d.app.pins.hits[0].1;
    for _ in 0..5 {
        d.app.on_scroll(rect.x + 1, rect.y + 1, true);
    }
    d.app.on_click(rect.x + 1, rect.y + 1);
    d.app.tick_pins();
    assert_eq!(fs::read(&file).unwrap(), before, "no write while moving");
    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
}

#[test]
fn leaving_the_sidebar_saves_its_place_once() {
    let (mut d, file) = pinned("pins-leave");
    d.app.tick_pins();
    let before = fs::read(&file).unwrap();
    // Move onto a card other than the one the file records.
    let on_file = grimoire_core::pins::Store::load(&d.root)
        .unwrap()
        .state
        .selected;
    for key in [KeyCode::Char('j'), KeyCode::Char('k')] {
        if d.app.pins.state().selected == on_file {
            d.key(key);
        }
    }
    let selected = d.app.pins.state().selected;
    assert_ne!(selected, on_file, "the selection moved off the file's");
    d.key(KeyCode::Tab);
    assert_ne!(d.app.focus, Focus::Pins);
    d.app.tick_pins();
    let after = fs::read(&file).unwrap();
    assert_ne!(after, before, "the new place is written");
    let reloaded = grimoire_core::pins::Store::load(&d.root).unwrap();
    assert_eq!(reloaded.state.selected, selected);
    d.app.tick_pins();
    assert_eq!(fs::read(&file).unwrap(), after, "once");
}

#[test]
fn the_wheel_over_the_pins_with_the_page_focused_writes_nothing() {
    let (mut d, file) = pinned("pins-wheel");
    d.key(KeyCode::Tab);
    d.app.tick_pins();
    if d.app.focus == Focus::Pins {
        d.key(KeyCode::Tab);
        d.app.tick_pins();
    }
    assert_ne!(d.app.focus, Focus::Pins);
    let before = fs::read(&file).unwrap();
    let rect = d.app.pins.hits[0].1;
    for _ in 0..5 {
        d.app.on_scroll(rect.x + 1, rect.y + 1, true);
        d.draw();
        d.app.tick_pins();
    }
    assert_eq!(fs::read(&file).unwrap(), before, "no write per wheel step");
}

#[test]
fn j_and_k_move_pins_and_home_still_returns_to_the_anchor() {
    let (mut d, _) = pinned("pins-jk");
    let ids = d.app.pin_ids(false);
    d.key(KeyCode::Home);
    let first = d.app.pins.state().selected;
    d.key(KeyCode::Char('j'));
    assert_ne!(d.app.pins.state().selected, first, "j moves down");
    d.key(KeyCode::Char('k'));
    assert_eq!(d.app.pins.state().selected, first, "k moves up");
    assert!(ids.len() >= 2);
}

#[test]
fn esc_in_the_pins_sidebar_hides_it_and_esc_in_the_editor_keeps_it() {
    let (mut d, _) = pinned("pins-esc");
    d.key(KeyCode::Esc);
    assert!(!d.app.pins.state().sidebar_visible, "Esc hides it");
    assert_ne!(d.app.focus, Focus::Pins);
    assert_eq!(d.app.overlay, Overlay::None, "and doesn't open the menu");
    d.app.toggle_pins();
    d.key(KeyCode::Tab);
    if d.app.focus == Focus::Pins {
        d.key(KeyCode::Tab);
    }
    d.key(KeyCode::Esc);
    assert!(
        d.app.pins.state().sidebar_visible,
        "Esc in the editor keeps it"
    );
}

#[test]
fn the_selected_pin_has_the_shaded_triangle() {
    let (d, _) = pinned("pins-marker");
    assert!(d.shows("▸ "), "{}", d.rows().join("\n"));
    assert!(!d.rows().iter().any(|r| r.contains('▶')));
}

#[test]
fn pinboard_shows_a_few_keys_and_the_rest_behind_question_mark() {
    let (mut d, _) = pinned("pins-help");
    d.app.open_pinboard();
    d.draw();
    assert!(d.shows("? all keys"), "{}", d.rows().join("\n"));
    assert!(
        !d.shows("Tab collection"),
        "the long list is on the help page"
    );
    assert!(d.shows("Pinboard"));
    assert!(!d.shows("metadata-only"));
}
