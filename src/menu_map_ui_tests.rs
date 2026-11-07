//! The Settings menu map (#90): every page is reachable by walking its groups by
//! name, has one place, and group pages stay short.
use super::*;
use crate::app::shell::{Route, menu, page_label, parent, path_to};

fn settings(tag: &str) -> Desk {
    let mut desk = Desk::open(book(tag, true), 120, 40);
    desk.app.deck.simulate();
    desk
}

/// A page that asks the system for something waits for the answer before the next key.
fn idle(desk: &mut Desk) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while desk.app.deck.busy && std::time::Instant::now() < end {
        desk.app.tick_deck();
        std::thread::yield_now();
    }
    desk.draw();
}

#[test]
fn every_page_on_the_map_is_reached_by_name_and_titled_by_its_path() {
    let mut desk = settings("menu-walk");
    for (_, pages) in menu() {
        for &page in *pages {
            desk.app.open_settings();
            desk.draw();
            for step in path_to(page) {
                idle(&mut desk);
                desk.key(KeyCode::Home);
                desk.choose(page_label(step));
            }
            // Sound and Local accounts open once the system has answered.
            for _ in 0..300 {
                if desk.app.deck.shell.nav.current() == page {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
                desk.app.tick_deck();
                desk.draw();
            }
            assert_eq!(desk.app.deck.shell.nav.current(), page, "{page:?}");
            let crumbs: Vec<&str> = path_to(page).into_iter().map(page_label).collect();
            let title = format!("Settings › {}", crumbs.join(" › "));
            assert!(
                desk.rows()[0].contains(&title),
                "{page:?}: {}",
                desk.rows()[0]
            );
            while desk.app.overlay != Overlay::None {
                desk.key(KeyCode::Esc);
            }
        }
    }
}

#[test]
fn each_page_has_one_place_and_group_pages_stay_short() {
    let mut seen = vec![];
    for (group, pages) in menu() {
        assert!(pages.len() <= 8, "{group:?} lists {} pages", pages.len());
        for page in *pages {
            assert!(!seen.contains(page), "{page:?} is in two groups");
            seen.push(*page);
            assert!(!page_label(*page).is_empty(), "{page:?} has no name");
        }
    }
    // Every group except Settings is itself on the map, so every page can be reached.
    for (group, _) in menu() {
        assert!(
            *group == Route::Settings || parent(*group).is_some(),
            "{group:?} hangs loose"
        );
    }
}

#[test]
fn back_retraces_the_maps_path_from_a_lesson() {
    use crate::app::shell::{Category, Panel};
    let mut desk = settings("menu-back");
    desk.app.open_settings_page(Route::Panel(Panel::Updates));
    desk.draw();
    assert_eq!(
        desk.app.deck.shell.nav.current(),
        Route::Panel(Panel::Updates)
    );
    desk.key(KeyCode::Esc);
    assert_eq!(
        desk.app.deck.shell.nav.current(),
        Route::Category(Category::Updates)
    );
    desk.key(KeyCode::Esc);
    assert_eq!(desk.app.deck.shell.nav.current(), Route::Settings);
}
