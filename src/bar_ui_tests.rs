//! The menu bar (rc13): its titles, every row rc12's Esc menu had, the keys,
//! the mouse, Super and Find anything, at every supported size.
use super::*;
use crate::palette::{Action as A, Side, Title};
use ratatui::crossterm::event::ModifierKeyCode;
use ratatui::layout::Rect;

fn labels(rows: &[crate::app::overlays::BarRow]) -> Vec<String> {
    rows.iter().map(|r| r.label.clone()).collect()
}

/// Every row of every menu and side group, with the menu it's in.
fn all_rows(app: &App) -> Vec<(String, crate::app::overlays::BarRow)> {
    let mut out = vec![];
    for title in Title::ALL {
        for row in app.bar_rows(title) {
            out.push((title.label().to_string(), row));
        }
    }
    for side in Side::ALL {
        for row in app.side_rows(side) {
            out.push((format!("Tools › {}", side.label()), row));
        }
    }
    out
}

/// A writer with a scene open, conflicts to settle, music on, something in
/// the Trash and echo words on: every conditional row is offered.
fn everything_offered(tag: &str) -> Desk {
    everything_offered_at(tag, 120, 40)
}

fn everything_offered_at(tag: &str, width: u16, height: u16) -> Desk {
    let (root, _, _) = conflicted(tag);
    let mut d = Desk::open(root, width, height);
    d.key(KeyCode::Tab);
    d.app.music.enabled = true;
    d.app.echo_on = true;
    d.app.deck.quit_offered = true;
    d.app.deck.live_session = true;
    d.draw();
    d
}

#[test]
fn the_bar_has_eight_titles_in_order() {
    let labels: Vec<&str> = Title::ALL.iter().map(|t| t.label()).collect();
    assert_eq!(
        labels,
        [
            "WrOS", "File", "Edit", "View", "Book", "Tools", "Settings", "Help"
        ]
    );
    let mut letters: Vec<char> = Title::ALL.iter().map(|t| t.letter()).collect();
    letters.sort();
    letters.dedup();
    assert_eq!(letters.len(), 8, "each title its own letter");
    let d = Desk::open(book("bar-line", true), 80, 24);
    assert!(
        d.rows()[0].starts_with("  WrOS  File  Edit  View  Book  Tools  Settings  Help"),
        "{:?}",
        d.rows()[0]
    );
}

#[test]
fn the_bar_titles_are_short_and_tools_has_three_side_groups() {
    let d = everything_offered("bar-short");
    for (menu, row) in all_rows(&d.app) {
        assert!(row.label.chars().count() <= 32, "{menu}: {}", row.label);
    }
    let tools = d.app.bar_rows(Title::Tools);
    let sides: Vec<&A> = tools
        .iter()
        .map(|r| &r.action)
        .filter(|a| matches!(a, A::Side(_)))
        .collect();
    assert_eq!(
        sides,
        [
            &A::Side(Side::Revision),
            &A::Side(Side::Notes),
            &A::Side(Side::Practice)
        ]
    );
    assert!(tools.iter().filter(|r| r.key == "›").count() == 3);
    let settings = labels(&d.app.bar_rows(Title::Settings));
    assert_eq!(
        settings,
        [
            "Writing",
            "Look and keyboard",
            "System",
            "Backups and accounts",
            "Updates and recovery"
        ]
    );
}

#[test]
fn no_dropdown_is_taller_than_twelve_rows_and_no_row_starts_with_j_or_k() {
    let d = everything_offered("bar-twelve");
    for title in Title::ALL {
        assert!(d.app.bar_rows(title).len() <= 12, "{}", title.label());
    }
    for side in Side::ALL {
        assert!(d.app.side_rows(side).len() <= 12, "{}", side.label());
    }
    for (menu, row) in all_rows(&d.app) {
        let first = row.label.chars().next().unwrap().to_ascii_lowercase();
        assert!(
            first != 'j' && first != 'k',
            "{menu}: {} (j and k move)",
            row.label
        );
    }
}

/// Every row rc12's Esc menu and its groups had (d438fe4), but the ones the
/// rc13 design retires: Find anything (Super or Ctrl+K), the Settings group
/// (all in Settings), the WrOS dashboard row (Overview) and the group doors.
const RC12_ROWS: &[A] = &[
    A::Conflicts,
    A::Export,
    A::NewBook,
    A::OpenBook,
    A::Quit,
    // This book
    A::Corkboard,
    A::NewScene,
    A::NewChapter,
    A::NewPart,
    A::NewFolder,
    A::Rename,
    A::Delete,
    A::Restore,
    A::MoveUp,
    A::MoveDown,
    A::History,
    A::FindInBook,
    A::Goals,
    A::Sessions,
    A::SaveSession,
    A::Compile,
    A::ProjectMap,
    A::MoveHistoryOut,
    // Writing tools
    A::FocusMode,
    A::Details,
    A::Italic,
    A::Bold,
    A::FindInScene,
    A::BesidePicker,
    A::NotesList,
    A::NextTk,
    A::OpenCodex,
    A::NextDraft,
    A::EchoWords,
    A::Footnotes,
    A::FootnoteAtCaret,
    A::InsertFootnote,
    A::CheckNames,
    A::Continuity,
    A::Milestones,
    A::ReviewNotes,
    A::WritingChecks,
    A::Lexicon,
    A::ProseDashboard,
    A::StartSprint,
    A::Progress,
    A::Timer,
    A::TimerReset,
    A::ReadAloud,
    A::ReadChapterAloud,
    A::ReadManuscriptAloud,
    A::MusicPlayer,
    A::ToggleSecondary,
    A::SecondaryPicker,
    A::Journal,
    A::Scratchpad,
    A::Handoff,
    A::Pinboard,
    A::PinCurrent,
    A::TogglePins,
    // Productivity tools
    A::StorySpark,
    A::Productivity,
    A::MeasuredGoals,
    A::Calendar,
    A::AdvancedSprints,
    A::Achievements,
    A::DailyChallenges,
    // Help & about
    A::Help,
    A::Tutorial,
    A::About,
    A::License,
    A::Donate,
];

#[test]
fn every_rc12_menu_row_has_a_place_on_the_bar() {
    let d = everything_offered("bar-rc12");
    let on_bar: Vec<A> = all_rows(&d.app)
        .into_iter()
        .map(|(_, r)| r.action)
        .collect();
    // Shown only with something selected in the Trash, or a synced book that
    // still keeps its history inside it: the same conditions as rc12's rows.
    let conditional = [A::Restore, A::MoveHistoryOut];
    let missing: Vec<&A> = RC12_ROWS
        .iter()
        .filter(|a| !on_bar.contains(a) && !conditional.contains(a))
        .collect();
    assert!(
        missing.is_empty(),
        "rc12 rows with no place on the bar: {missing:?}"
    );
    assert!(
        on_bar.contains(&A::Dashboard),
        "Overview stands in for the dashboard row"
    );
}

#[test]
fn every_rc12_settings_group_row_lives_in_wros_settings_or_on_the_bar() {
    let d = everything_offered("bar-settings-rows");
    let on_bar: Vec<A> = all_rows(&d.app)
        .into_iter()
        .map(|(_, r)| r.action)
        .collect();
    let mut in_settings = vec![];
    for (group, pages) in crate::app::shell::menu() {
        in_settings.extend(crate::app::shell::page_actions(&d.app, *group));
        for page in *pages {
            in_settings.extend(crate::app::shell::page_actions(&d.app, *page));
        }
    }
    for (label, action) in d.app.setting_rows() {
        if matches!(action, A::Settings | A::SystemSettings) {
            continue; // the Settings menu itself replaces these
        }
        assert!(
            in_settings.contains(&action) || on_bar.contains(&action),
            "{label} has nowhere to live"
        );
    }
}

#[test]
fn every_bar_row_says_what_it_does_and_has_a_help_topic() {
    let d = everything_offered("bar-about");
    for (menu, row) in all_rows(&d.app) {
        let (says, topic) = crate::app::overlays::about_row(&row.action, "part");
        assert!(!says.is_empty(), "{menu}: {} says nothing", row.label);
        assert!(!says.contains('—'), "{menu}: {}: no em dashes", row.label);
        assert_eq!(
            crate::help::TOPICS[crate::help::index(topic)].id,
            topic,
            "{}",
            row.label
        );
    }
}

#[test]
fn nothing_in_ctrl_k_is_missing_from_the_bar() {
    let d = everything_offered("bar-ctrlk");
    let on_bar: Vec<A> = all_rows(&d.app)
        .into_iter()
        .map(|(_, r)| r.action)
        .collect();
    let missing: Vec<String> = palette::entries(&d.app)
        .into_iter()
        .filter(|e| {
            !matches!(
                e.action,
                A::Save | A::Menu | A::SpellingSuggestions
                    // The music pane's own keys.
                    | A::PlayPause | A::NextTrack | A::PrevTrack
                    // Things by name: scenes, notes, themes, books, topics.
                    | A::Open(_) | A::Theme(_) | A::OpenBookAt(_) | A::Beside(_) | A::HelpTopic(_)
                    // Offered where you'd look for them: Open another book.
                    | A::BringInDraft
                    // Settings rows, reached through the Settings menu.
                    | A::Settings | A::SystemSettings | A::MusicToggle | A::MusicSource
                    | A::Spellcheck | A::SpellingLanguage | A::Icons | A::LineWidth
                    | A::Typewriter | A::Voices | A::ManuscriptLook | A::AuthorDetails
            )
        })
        .filter(|e| !on_bar.contains(&e.action))
        .map(|e| e.label)
        .collect();
    assert!(missing.is_empty(), "only in Find anything: {missing:#?}");
}

#[test]
fn every_bar_row_is_in_find_anything() {
    let d = everything_offered("bar-reverse");
    let found: Vec<A> = palette::entries(&d.app)
        .into_iter()
        .map(|e| e.action)
        .collect();
    let missing: Vec<String> = all_rows(&d.app)
        .into_iter()
        .filter(|(_, r)| !matches!(r.action, A::Side(_) | A::SettingsGroup(_)))
        .filter(|(_, r)| !found.contains(&r.action))
        .map(|(menu, r)| format!("{menu} › {}", r.label))
        .collect();
    assert!(
        missing.is_empty(),
        "on the bar but not in Find anything: {missing:#?}"
    );
}

#[test]
fn bar_path_names_the_menu() {
    let d = everything_offered("bar-path");
    assert_eq!(
        d.app.bar_path(&A::Pinboard).as_deref(),
        Some("Tools › Notes")
    );
    assert_eq!(d.app.bar_path(&A::NewScene).as_deref(), Some("Book"));
    assert_eq!(d.app.bar_path(&A::Dashboard).as_deref(), Some("WrOS"));
}

#[test]
fn esc_lights_the_bar_on_file_without_opening_it_and_esc_closes_it() {
    let mut d = Desk::open(book("bar-esc", true), 120, 35);
    d.key(KeyCode::Esc);
    assert_eq!(
        d.app.overlay,
        Overlay::Bar {
            title: Title::File,
            open: false,
            sel: 0,
            side: None
        }
    );
    assert!(!d.shows("Start a new book…"), "lit, not dropped down");
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None);
}

#[test]
fn esc_clears_a_selection_before_opening_the_bar() {
    let mut d = Desk::open(book("bar-esc-sel", true), 120, 35);
    d.key(KeyCode::Tab);
    d.typed("The lamp was lit.");
    d.ctrl('a');
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None);
    assert!(d.app.editor.selected_text().is_none());
    d.key(KeyCode::Esc);
    assert!(d.menu_open());
}

#[test]
fn esc_then_a_letter_opens_that_menu_on_the_console_too() {
    let mut d = Desk::open(book("bar-letters", true), 100, 30);
    d.app.linux_console = true;
    for (letter, title) in [('b', Title::Book), ('h', Title::Help), ('W', Title::Wros)] {
        d.key(KeyCode::Esc);
        d.key(KeyCode::Char(letter));
        assert!(
            matches!(d.app.overlay, Overlay::Bar { title: t, open: true, .. } if t == title),
            "{letter}: {:?}",
            d.app.overlay
        );
        assert!(d.shows(&format!("▸ {}", d.app.bar_rows(title)[0].label)));
        d.key(KeyCode::Esc);
    }
}

#[test]
fn arrows_walk_the_titles_and_down_drops_the_menu() {
    let mut d = Desk::open(book("bar-arrows", true), 120, 35);
    d.key(KeyCode::Esc);
    d.key(KeyCode::Left);
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            title: Title::Wros,
            open: false,
            ..
        }
    ));
    d.key(KeyCode::Left);
    assert!(
        matches!(
            d.app.overlay,
            Overlay::Bar {
                title: Title::Help,
                open: false,
                ..
            }
        ),
        "round"
    );
    d.key(KeyCode::Down);
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            title: Title::Help,
            open: true,
            ..
        }
    ));
    d.key(KeyCode::Right);
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            title: Title::Wros,
            open: true,
            ..
        }
    ));
    assert!(d.shows("▸ Overview"));
}

#[test]
fn f10_opens_and_closes_the_bar() {
    let mut d = Desk::open(book("bar-f10", true), 120, 35);
    d.key(KeyCode::F(10));
    assert!(d.menu_open());
    d.key(KeyCode::F(10));
    assert_eq!(d.app.overlay, Overlay::None);
}

#[test]
fn alt_and_a_letter_opens_that_menu_in_the_graphical_session_only() {
    let mut d = Desk::open(book("bar-alt", true), 120, 35);
    d.app.alt_menus = true;
    d.press(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT));
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            title: Title::View,
            open: true,
            ..
        }
    ));
    d.key(KeyCode::Esc);
    d.app.alt_menus = false;
    d.press(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT));
    assert_eq!(
        d.app.overlay,
        Overlay::None,
        "a terminal keeps Alt for itself"
    );
}

#[test]
fn a_letter_in_a_dropdown_jumps_to_its_row_and_enter_runs_it() {
    let mut d = Desk::open(book("bar-jump", true), 120, 35);
    d.key(KeyCode::Esc);
    d.key(KeyCode::Char('b'));
    d.key(KeyCode::Char('r'));
    assert!(d.shows("▸ Rename…"), "{}", d.rows().join("\n"));
    d.key(KeyCode::Char('n'));
    assert!(d.shows("▸ New scene…"), "round to the first n");
    d.key(KeyCode::Enter);
    assert!(matches!(d.app.overlay, Overlay::Create { .. }));
}

#[test]
fn tools_side_groups_open_to_the_side_and_left_comes_back() {
    let mut d = Desk::open(book("bar-side", true), 120, 35);
    d.key(KeyCode::Tab);
    d.key(KeyCode::Esc);
    d.key(KeyCode::Char('t'));
    d.choose("Notes");
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            title: Title::Tools,
            side: Some((Side::Notes, 0)),
            ..
        }
    ));
    assert!(d.shows("▸ Pinboard…"), "{}", d.rows().join("\n"));
    assert!(d.shows("Notes"), "the Tools menu stays beside it");
    d.key(KeyCode::Left);
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            side: None,
            open: true,
            ..
        }
    ));
    d.menu(&["Tools", "Revision", "Revision notes…"]);
    assert!(
        matches!(d.app.overlay, Overlay::ReviewNotes),
        "{:?}",
        d.app.overlay
    );
}

#[test]
fn the_highlighted_rows_description_is_on_the_status_line_and_question_mark_opens_its_page() {
    let mut d = Desk::open(book("bar-explain", true), 120, 35);
    d.key(KeyCode::Tab);
    d.key(KeyCode::Esc);
    d.key(KeyCode::Char('t'));
    d.choose("Revision");
    for _ in 0..12 {
        if d.shows("▸ Turn echo words on") {
            break;
        }
        d.key(KeyCode::Down);
    }
    assert!(
        d.status().contains("like \"the lamp… the lamp\""),
        "{}",
        d.status()
    );
    d.key(KeyCode::Char('?'));
    assert_eq!(help_topic(&d), Some("revision"));
    d.key(KeyCode::Esc);
    assert!(
        matches!(
            d.app.overlay,
            Overlay::Bar {
                side: Some((Side::Revision, _)),
                ..
            }
        ),
        "closing the help comes back to the menu"
    );
}

#[test]
fn every_menu_shows_each_row_and_its_explanation_at_every_supported_size() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50), (170, 48)] {
        let mut d = everything_offered_at(&format!("bar-sizes-{width}"), width, height);
        for title in Title::ALL {
            d.app.open_bar(Some(title), true);
            d.draw();
            let rows = d.app.bar_rows(title);
            for (i, row) in rows.iter().enumerate() {
                if let Overlay::Bar { sel, .. } = &mut d.app.overlay {
                    *sel = i;
                }
                d.draw();
                assert!(
                    d.shows(&format!("▸ {}", row.label)),
                    "{} › {} at {width}x{height}\n{}",
                    title.label(),
                    row.label,
                    d.rows().join("\n")
                );
                let (says, _) = crate::app::overlays::about_row(&row.action, "part");
                let says = row.dim.map_or(says, str::to_string);
                let start: String = says
                    .split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ");
                assert!(
                    d.status().contains(&start),
                    "{} at {width}x{height}: {}",
                    row.label,
                    d.status()
                );
            }
        }
        d.app.close_bar();
    }
}

#[test]
fn dropdowns_fit_or_scroll_at_80x24_and_keep_the_status_line() {
    for (width, height) in [(80, 24), (80, 16)] {
        let mut d = everything_offered_at(&format!("bar-fit-{height}"), width, height);
        for title in Title::ALL {
            d.app.open_bar(Some(title), true);
            d.key(KeyCode::Up); // round to the last row
            let last = d.app.bar_rows(title).last().unwrap().label.clone();
            assert!(
                d.shows(&format!("▸ {last}")),
                "{} at {width}x{height}",
                title.label()
            );
            for (r, _) in d.app.bar_hits.borrow().iter() {
                assert!(
                    r.right() <= width && r.bottom() < height,
                    "{r:?} at {width}x{height}"
                );
            }
        }
    }
}

#[test]
fn dim_rows_cannot_be_chosen_and_say_why() {
    let mut d = Desk::open(book("bar-dim", true), 120, 35);
    d.app.deck.suspend_available = false;
    d.key(KeyCode::Esc);
    d.key(KeyCode::Char('w'));
    d.choose("Sleep");
    assert!(d.menu_open(), "nothing ran");
    assert!(
        d.status().contains(&wros_sys::SUSPEND_UNAVAILABLE[..20]),
        "{}",
        d.status()
    );
    // Without a scene open, the editing rows are dim and say so.
    d.app.open = None;
    d.app.open_bar(Some(Title::Edit), true);
    d.draw();
    assert!(d.status().contains("Open a scene first"), "{}", d.status());
}

#[test]
fn the_bar_costs_one_page_line_and_focus_mode_hides_it() {
    let mut d = Desk::open(book("bar-focus", true), 120, 35);
    d.key(KeyCode::Tab);
    // The outline's border sits on the line under the bar; its rows below it.
    assert_eq!(d.app.rect_tree.y, 2, "the page starts under the bar");
    d.ctrl('d');
    assert!(d.app.focus_mode);
    assert!(!d.rows()[0].contains("File  Edit"), "{:?}", d.rows()[0]);
    d.key(KeyCode::Esc);
    assert!(
        d.rows()[0].contains("File  Edit"),
        "Esc draws it over the top line"
    );
    d.key(KeyCode::Esc);
    assert!(!d.rows()[0].contains("File  Edit"));
}

#[test]
fn a_super_tap_opens_find_anything_under_the_lit_bar() {
    let mut d = Desk::open(book("bar-super", true), 120, 35);
    d.press(KeyEvent::new(
        KeyCode::Modifier(ModifierKeyCode::LeftSuper),
        KeyModifiers::NONE,
    ));
    assert!(matches!(
        d.app.overlay,
        Overlay::Palette {
            under_bar: true,
            ..
        }
    ));
    assert!(
        d.rows()[1].contains("FIND ANYTHING"),
        "just under the bar: {:?}",
        d.rows()[1]
    );
    d.key(KeyCode::Left);
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            title: Title::Help,
            open: true,
            ..
        }
    ));
    d.key(KeyCode::Esc);
    d.press(KeyEvent::new(
        KeyCode::Modifier(ModifierKeyCode::LeftSuper),
        KeyModifiers::NONE,
    ));
    d.key(KeyCode::Right);
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            title: Title::Wros,
            open: true,
            ..
        }
    ));
    d.key(KeyCode::Esc);
    d.press(KeyEvent::new(
        KeyCode::Modifier(ModifierKeyCode::LeftSuper),
        KeyModifiers::NONE,
    ));
    d.press(KeyEvent::new(
        KeyCode::Modifier(ModifierKeyCode::LeftSuper),
        KeyModifiers::NONE,
    ));
    assert_eq!(d.app.overlay, Overlay::None, "a second tap closes it");
}

#[test]
fn super_chord_types_nothing() {
    let mut d = Desk::open(book("bar-super-chord", true), 120, 35);
    d.key(KeyCode::Tab);
    let before = d.app.editor.text();
    d.press(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    d.press(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::SUPER));
    assert_eq!(d.app.editor.text(), before);
    assert_eq!(d.app.overlay, Overlay::None);
    assert_eq!(d.app.mod_label(), "^", "no ⌘ labels on WrOS");
}

#[test]
fn find_anything_names_each_rows_menu() {
    let mut d = Desk::open(book("bar-find-path", true), 120, 35);
    d.ctrl('k');
    d.typed("pinboard");
    let row = d
        .rows()
        .into_iter()
        .find(|r| r.contains("▸ Pinboard"))
        .unwrap_or_default();
    assert!(row.contains("Tools › Notes"), "{row}");
}

#[test]
fn clicking_a_title_opens_it_hovering_switches_and_clicking_away_closes() {
    let mut d = Desk::open(book("bar-mouse", true), 120, 35);
    let at = |d: &Desk, title: Title| {
        d.app
            .bar_hits
            .borrow()
            .iter()
            .find(|(_, h)| *h == crate::app::overlays::BarHit::Title(title))
            .map(|(r, _)| (r.x + 1, r.y))
            .unwrap()
    };
    let (x, y) = at(&d, Title::Book);
    d.app.on_click(x, y);
    d.draw();
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            title: Title::Book,
            open: true,
            ..
        }
    ));
    let (x, y) = at(&d, Title::View);
    d.app.bar_hover(x, y);
    d.draw();
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            title: Title::View,
            open: true,
            ..
        }
    ));
    // Hover lights a row; the wheel moves it; a click runs it.
    let rows: Vec<(Rect, crate::app::overlays::BarHit)> = d.app.bar_hits.borrow().clone();
    let (third, _) = rows
        .iter()
        .find(|(_, h)| *h == crate::app::overlays::BarHit::Row(2))
        .unwrap();
    d.app.bar_hover(third.x + 2, third.y);
    assert!(matches!(d.app.overlay, Overlay::Bar { sel: 2, .. }));
    d.app.on_scroll(third.x + 2, third.y, true);
    assert!(matches!(d.app.overlay, Overlay::Bar { sel: 3, .. }));
    // A click anywhere else closes the menu and reaches nothing under it.
    let sel = d.app.sel;
    d.app.on_click(5, 20);
    assert_eq!(d.app.overlay, Overlay::None);
    assert_eq!(d.app.sel, sel);
    // A click on a row runs it.
    let (x, y) = at(&d, Title::Help);
    d.app.on_click(x, y);
    d.draw();
    let (row, _) = *d
        .app
        .bar_hits
        .borrow()
        .iter()
        .find(|(_, h)| *h == crate::app::overlays::BarHit::Row(0))
        .unwrap();
    d.app.on_click(row.x + 2, row.y);
    assert!(
        matches!(d.app.overlay, Overlay::Help { .. }),
        "{:?}",
        d.app.overlay
    );
}

#[test]
fn side_groups_take_the_mouse_too() {
    let mut d = Desk::open(book("bar-side-mouse", true), 120, 35);
    d.app.open_bar(Some(Title::Tools), true);
    d.draw();
    let notes = d
        .app
        .bar_rows(Title::Tools)
        .iter()
        .position(|r| r.action == A::Side(Side::Notes))
        .unwrap();
    let hit_of = |d: &Desk, want: crate::app::overlays::BarHit| {
        d.app
            .bar_hits
            .borrow()
            .iter()
            .find(|(_, h)| *h == want)
            .map(|(r, _)| *r)
            .unwrap()
    };
    let r = hit_of(&d, crate::app::overlays::BarHit::Row(notes));
    d.app.on_click(r.x + 2, r.y);
    d.draw();
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            side: Some((Side::Notes, 0)),
            ..
        }
    ));
    let r = hit_of(&d, crate::app::overlays::BarHit::SideRow(2));
    d.app.bar_hover(r.x + 2, r.y);
    assert!(matches!(
        d.app.overlay,
        Overlay::Bar {
            side: Some((Side::Notes, 2)),
            ..
        }
    ));
    d.app.on_click(r.x + 2, r.y);
    assert_eq!(d.app.overlay, Overlay::Journal);
}

#[test]
fn settings_rows_open_the_settings_box_on_their_group() {
    let mut d = Desk::open(book("bar-settings", true), 120, 35);
    d.menu(&["Settings", "System"]);
    assert_eq!(
        d.app.overlay,
        Overlay::Deck(crate::app::wros::Page::Settings)
    );
    assert_eq!(
        d.app.deck.shell.nav.current(),
        crate::app::shell::Route::Category(crate::app::shell::Category::System)
    );
    assert!(d.shows("Battery and power"), "{}", d.rows().join("\n"));
}

#[test]
fn a_setting_switched_from_a_menu_keeps_it_open() {
    let mut d = Desk::open(book("bar-toggle", true), 120, 35);
    d.key(KeyCode::Tab);
    d.menu(&["Tools", "Revision", "Turn echo words on"]);
    assert!(d.app.echo_on);
    assert!(d.shows("▸ Turn echo words off"), "{}", d.rows().join("\n"));
}

#[test]
fn no_grimoire_name_on_a_screen_the_bar_opens() {
    let mut d = everything_offered("bar-brand");
    d.app.deck.live_session = false;
    let rows: Vec<(String, crate::app::overlays::BarRow)> = all_rows(&d.app);
    for (menu, row) in rows {
        // The credit, the license and the Donate screen keep the name, as does
        // the theme called Grimoire (saved theme settings name it). Rows that
        // reach the computer or leave are not opened here.
        if matches!(
            row.action,
            A::About
                | A::License
                | A::Donate
                | A::Themes
                | A::Help
                | A::Quit
                | A::Lock
                | A::Sleep
                | A::Restart
                | A::ShutDown
                | A::InstallToDisk
                | A::BackUpNow
                | A::ReadAloud
                | A::ReadChapterAloud
                | A::ReadManuscriptAloud
                | A::Side(_)
                | A::SettingsGroup(_)
        ) {
            continue;
        }
        d.app.overlay = Overlay::None;
        d.app.run_action(row.action.clone());
        d.draw();
        // The test book is named after its temporary folder, grimoire-ui-…
        let screen = d
            .rows()
            .join("\n")
            .replace("Grimoire Ui", "")
            .replace("GRIMOIRE UI", "");
        assert!(
            !screen.contains("Grimoire"),
            "{menu} › {}:\n{screen}",
            row.label
        );
        for _ in 0..4 {
            d.key(KeyCode::Esc);
        }
        d.app.focus_mode = false;
    }
}

#[test]
fn about_credits_grimoiretui_and_names_mit_and_the_outline_has_no_book_art() {
    let mut d = Desk::open(book("bar-about", true), 160, 50);
    assert!(!d.shows("☾"), "no spellbook in the outline");
    d.menu(&["WrOS", "About WrOS"]);
    assert!(d.shows("About WrOS"), "{}", d.rows().join("\n"));
    assert!(d.shows("built on GrimoireTUI"));
    assert!(d.shows("Josh King"));
    assert!(d.shows("Catfinity Studios"));
    assert!(d.shows("MIT License"));
    assert!(d.shows("###"), "the WrOS logo heads it");
}

/// Does the path `segments` (after its menu title) lead somewhere real?
fn resolves(app: &App, title: Title, segments: &[&str]) -> Result<(), String> {
    use crate::app::shell::{Category, Route, menu, page_label, page_labels};
    // "Turn typewriter scrolling off" names a row whose end says its state.
    let like = |label: &str, said: &str| {
        let words = |s: &str| -> Vec<String> {
            s.trim_end_matches(['…', '›', ' '])
                .split_whitespace()
                .map(|w| w.trim_matches([':', '…']).to_lowercase())
                // A row that shows or hides names the state it would change to.
                .map(|w| if w == "hide" { "show".to_string() } else { w })
                .collect()
        };
        let (label, mut said) = (words(label), words(said));
        // A switch's row says its other state ("Turn music off" for "on").
        if said.len() > 1 && matches!(said.last().map(String::as_str), Some("on" | "off")) {
            said.pop();
        }
        let n = said.len().min(3);
        n > 0 && label.len() >= n && label[..n] == said[..n]
    };
    let Some((first, rest)) = segments.split_first() else {
        return Ok(());
    };
    if title == Title::Settings {
        let group = [
            Category::Writing,
            Category::Interface,
            Category::System,
            Category::Accounts,
            Category::Updates,
        ]
        .into_iter()
        .find(|c| like(c.label(), first))
        .ok_or_else(|| format!("no Settings group {first}"))?;
        let mut route = Route::Category(group);
        for seg in rest {
            let pages: Vec<Route> = menu()
                .iter()
                .find(|(g, _)| *g == route)
                .map(|(_, pages)| pages.to_vec())
                .unwrap_or_default();
            if let Some(page) = pages.iter().find(|r| like(page_label(**r), seg)) {
                route = *page;
                continue;
            }
            if page_labels(app, route).iter().any(|l| like(l, seg)) {
                return Ok(()); // a row on the page; what it opens is its own
            }
            return Err(format!("{seg} is not on {:?}", route));
        }
        return Ok(());
    }
    let row = app
        .bar_rows(title)
        .into_iter()
        .find(|r| like(&r.label, first))
        .ok_or_else(|| format!("no {first} in {}", title.label()))?;
    if let (A::Side(side), Some(next)) = (&row.action, rest.first()) {
        app.side_rows(*side)
            .iter()
            .find(|r| like(&r.label, next))
            .ok_or_else(|| format!("no {next} in {}", side.label()))?;
    }
    Ok(())
}

#[test]
fn every_path_in_help_and_the_tutorial_is_a_real_menu_path() {
    let d = everything_offered("bar-help-paths");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut texts: Vec<(String, String)> = std::fs::read_dir(root.join("assets/help"))
        .unwrap()
        .flatten()
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read_to_string(e.path()).unwrap(),
            )
        })
        .collect();
    texts.push((
        "tutorial.json".into(),
        std::fs::read_to_string(root.join("crates/grimoire-core/assets/wros/tutorial.json"))
            .unwrap(),
    ));
    let titles = Title::ALL.map(|t| t.label());
    let mut wrong = vec![];
    let mut seen = 0;
    for (name, text) in &texts {
        assert!(
            !text.contains("Esc ›"),
            "{name} still names the old Esc menu"
        );
        // Paths read "Title › Row › …": cut each at the first character that
        // can't be part of a name.
        for (at, _) in text.match_indices(" › ") {
            let start = text[..at]
                .rfind(|c: char| !(c.is_alphanumeric() || c == ' ' || c == '-'))
                .map_or(0, |i| i + text[i..].chars().next().unwrap().len_utf8());
            let head = text[start..at].trim();
            let Some(title) = titles.iter().position(|t| head.ends_with(t)) else {
                continue;
            };
            // Only paths that begin at a title, not the middle of one.
            if !head.ends_with(&format!(" {}", titles[title])) && head != titles[title] {
                continue;
            }
            if text[..start].ends_with(" › ") {
                continue;
            }
            let tail = &text[at + " › ".len()..];
            let end = tail
                .find(['*', '.', ',', '(', ')', '"', ';', ':', '\n', '`'])
                .unwrap_or(tail.len());
            let path: Vec<&str> = tail[..end]
                .split(" › ")
                .map(|s| {
                    // "Keyboard, then …" or "Updates and choose …" end a path.
                    let s = s.split(" or ").next().unwrap();
                    let s = s.split(" in the ").next().unwrap();
                    let s = s.split(" and choose").next().unwrap();
                    let s = s.split(" and turn").next().unwrap();
                    let s = s
                        .split(" and ")
                        .next()
                        .filter(|_| {
                            !s.starts_with("Look and")
                                && !s.starts_with("Backups and")
                                && !s.starts_with("Updates and")
                                && !s.starts_with("Calendar and")
                                && !s.starts_with("Dictionary and")
                                && !s.starts_with("Prose and")
                                && !s.starts_with("Notes and")
                                && !s.starts_with("Battery and")
                                && !s.starts_with("Date and")
                                && !s.starts_with("Sound and")
                                && !s.starts_with("Wi-Fi and")
                        })
                        .unwrap_or(s);
                    s.trim()
                })
                .collect();
            // Other programs' own menus, described as steps in them.
            let elsewhere = ["Download", "Privacy & Security"];
            if path.first().is_some_and(|p| elsewhere.contains(p)) {
                continue;
            }
            seen += 1;
            if let Err(e) = resolves(&d.app, Title::ALL[title], &path) {
                wrong.push(format!(
                    "{name}: {} › {}: {e}",
                    titles[title],
                    path.join(" › ")
                ));
            }
        }
    }
    assert!(seen > 20, "found {seen} paths");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
