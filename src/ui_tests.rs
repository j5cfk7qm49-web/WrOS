//! The whole desk, driven the way a writer drives it: keys go through the same
//! routing as the running app (`on_key`), and every press is followed by a
//! real frame drawn into ratatui's test backend, so what's asserted is what
//! would be on screen. Each test gets a book of its own in a temp dir, and
//! nothing reads or writes ~/.config.

use super::*;
use crate::app::Setup;
use crate::palette::Sub;
use grimoire_core::recovery;
use grimoire_core::settings::Settings;
use grimoire_core::spell;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;
use std::fs;

struct Desk {
    app: App,
    term: Terminal<TestBackend>,
    armed: bool,
    left: bool,
    root: PathBuf,
}

/// A new book, scaffolded like `grimoire new`. `seen` makes it look opened
/// before, so it launches on its first scene rather than the guide.
fn book(tag: &str, seen: bool) -> PathBuf {
    let root = std::env::temp_dir().join(format!("grimoire-ui-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    project::scaffold(&root).unwrap();
    if seen {
        fs::create_dir_all(root.join(".grimoire")).unwrap();
        fs::write(root.join(".grimoire/progress.toml"), "").unwrap();
    }
    root
}

fn first_scene(root: &Path) -> PathBuf {
    let mut ch = root.join("manuscript");
    for _ in 0..2 {
        let mut dirs: Vec<PathBuf> = fs::read_dir(&ch)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        ch = dirs[0].clone();
    }
    ch.join("01-Scene-One.md")
}

impl Desk {
    fn open(root: PathBuf, w: u16, h: u16) -> Desk {
        let setup = Setup {
            music: music::Config::default(),
            theme: theme::default_theme(),
            settings: Settings::default(),
            background: false,
        };
        let app = App::with(Project::load(&root).unwrap(), setup).unwrap();
        let mut d = Desk {
            app,
            term: Terminal::new(TestBackend::new(w, h)).unwrap(),
            armed: false,
            left: false,
            root,
        };
        d.draw();
        d
    }

    fn draw(&mut self) {
        let app = &mut self.app;
        self.term.draw(|f| ui::draw(f, app)).unwrap();
    }

    fn press(&mut self, k: KeyEvent) {
        if on_key(&mut self.app, k, &mut self.armed) {
            self.left = true;
        }
        self.draw();
    }

    fn key(&mut self, code: KeyCode) {
        self.press(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn keys(&mut self, codes: &[KeyCode]) {
        for &c in codes {
            self.key(c);
        }
    }

    fn ctrl(&mut self, c: char) {
        self.press(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
    }

    fn typed(&mut self, s: &str) {
        for c in s.chars() {
            self.key(KeyCode::Char(c));
        }
    }

    fn rows(&self) -> Vec<String> {
        let buf = self.term.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn shows(&self, s: &str) -> bool {
        self.rows().iter().any(|r| r.contains(s))
    }

    fn status(&self) -> String {
        self.rows().last().cloned().unwrap_or_default()
    }

    fn menu_open(&self) -> bool {
        matches!(self.app.overlay, Overlay::Menu { .. })
    }

    /// Down the menu screen that's up to the row starting `label`, and ↵.
    fn choose(&mut self, label: &str) {
        for _ in 0..40 {
            if self.shows(&format!("▸ {label}")) {
                self.key(KeyCode::Enter);
                return;
            }
            self.key(KeyCode::Down);
        }
        panic!("no {label} row:\n{}", self.rows().join("\n"));
    }
}

impl Drop for Desk {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// ---- Esc and the menu ---------------------------------------------------

#[test]
fn esc_from_the_tree_opens_the_menu_and_esc_closes_it() {
    let mut d = Desk::open(book("esc-tree", true), 120, 35);
    assert_eq!(d.app.focus, Focus::Tree);
    d.key(KeyCode::Esc);
    assert!(d.menu_open());
    assert!(d.shows("Quit Grimoire"));
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None);
    assert!(!d.shows("Quit Grimoire"));
    assert_eq!(d.app.focus, Focus::Tree);
}

#[test]
fn closing_the_menu_leaves_you_in_the_editor() {
    let mut d = Desk::open(book("esc-editor", true), 120, 35);
    d.key(KeyCode::Tab);
    assert_eq!(d.app.focus, Focus::Editor);
    d.key(KeyCode::Esc);
    assert!(d.menu_open());
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None);
    assert_eq!(d.app.focus, Focus::Editor);
    assert!(d.status().contains("F8 spelling"), "{}", d.status());
}

#[test]
fn esc_clears_a_selection_before_it_opens_the_menu() {
    let mut d = Desk::open(book("esc-sel", true), 120, 35);
    d.key(KeyCode::Tab);
    d.typed("Some words");
    // Drag across the first word, as with the mouse.
    let r = d.app.rect_editor;
    d.app.on_click(r.x, r.y);
    d.app.on_drag(r.x + 4, r.y);
    d.draw();
    assert!(d.app.editor.has_selection());
    d.key(KeyCode::Esc);
    assert!(!d.app.editor.has_selection());
    assert!(!d.menu_open());
    d.key(KeyCode::Esc);
    assert!(d.menu_open());
}

#[test]
fn quit_grimoire_saves_and_leaves() {
    let root = book("quit", true);
    let scene = first_scene(&root);
    let mut d = Desk::open(root, 120, 35);
    d.key(KeyCode::Tab);
    d.typed("Zyx marker");
    d.key(KeyCode::Esc);
    // Up from the first row wraps to the last: Quit Grimoire.
    d.key(KeyCode::Up);
    assert!(d.shows("▸ Quit Grimoire"));
    d.key(KeyCode::Enter);
    assert!(d.left, "Enter on Quit Grimoire leaves");
    let saved = fs::read_to_string(&scene).unwrap();
    assert!(saved.contains("Zyx marker"), "{saved}");
}

#[test]
fn the_menu_acts_on_the_scene_being_written() {
    let mut d = Desk::open(book("menu-target", true), 120, 35);
    // The tree's cursor wanders off to Scene Three; then back to writing.
    d.keys(&[KeyCode::Down, KeyCode::Down]);
    d.key(KeyCode::Tab);
    d.key(KeyCode::Esc);
    d.choose("This book");
    d.choose("Delete…");
    match &d.app.overlay {
        Overlay::Confirm { name, .. } => assert_eq!(name, "Scene One"),
        other => panic!("expected the delete prompt, got {other:?}"),
    }
}

#[test]
fn the_first_menu_is_short_and_the_rest_is_one_step_down() {
    let d = Desk::open(book("menu-rows", true), 120, 35);
    let labels = |rows: Vec<(String, palette::Action)>| -> Vec<String> {
        rows.into_iter().map(|(l, _)| l).collect()
    };
    let top = labels(d.app.menu());
    assert!(top.len() <= 10, "{top:?}");
    for want in [
        "Find anything…",
        "This book",
        "Writing tools",
        "Export…",
        "Start a new book…",
        "Open another book…",
        "Settings",
        "Help & about",
    ] {
        assert!(
            top.iter().any(|l| l.starts_with(want)),
            "no {want} in {top:?}"
        );
    }
    assert!(top.last().unwrap().starts_with("Quit Grimoire"));
    let writing = labels(d.app.submenu(Sub::Writing));
    for want in [
        "Focus mode",
        "Open a scene beside…",
        "Notes & TKs…",
        "Next scene still in draft",
        "Turn echo words on",
        "Start a sprint…",
    ] {
        assert!(
            writing.iter().any(|l| l.starts_with(want)),
            "no {want} in {writing:?}"
        );
    }
    assert!(
        !writing.iter().any(|l| l.contains("Music player")),
        "music is off"
    );
    for sub in [
        Sub::Book,
        Sub::Writing,
        Sub::Productivity,
        Sub::Settings,
        Sub::Help,
    ] {
        assert_eq!(labels(d.app.submenu(sub)).last().unwrap(), "Back");
    }
}

#[test]
fn every_menu_row_says_what_it_does_and_has_a_help_topic() {
    let d = Desk::open(book("menu-about", true), 120, 35);
    let mut rows = d.app.menu();
    for sub in [
        Sub::Book,
        Sub::Writing,
        Sub::Productivity,
        Sub::Settings,
        Sub::Help,
    ] {
        rows.extend(d.app.submenu(sub));
    }
    for (label, action) in rows {
        let (says, topic) = crate::app::overlays::about_row(&action, "part");
        assert!(!says.is_empty(), "{label} says nothing");
        assert!(!says.contains('—'), "{label}: no em dashes");
        assert_eq!(
            crate::help::TOPICS[crate::help::index(topic)].id,
            topic,
            "{label}"
        );
    }
}

#[test]
fn the_highlighted_row_explains_itself_and_question_mark_opens_its_page() {
    let mut d = Desk::open(book("menu-explain", true), 120, 35);
    d.key(KeyCode::Tab);
    d.key(KeyCode::Esc);
    d.choose("Writing tools");
    for _ in 0..10 {
        if d.shows("▸ Turn echo words on") {
            break;
        }
        d.key(KeyCode::Down);
    }
    assert!(
        d.shows("like \"the lamp… the lamp\""),
        "{}",
        d.rows().join("\n")
    );
    d.key(KeyCode::Char('?'));
    assert_eq!(help_topic(&d), Some("revision"));
    d.key(KeyCode::Esc);
    assert!(
        matches!(
            d.app.overlay,
            Overlay::Sub {
                sub: Sub::Writing,
                ..
            }
        ),
        "closing the help comes back to the group"
    );
}

#[test]
fn writing_menu_keeps_selected_action_explanation_and_help_at_all_supported_sizes() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = Desk::open(book("menu-scroll-about", true), width, height);
        d.key(KeyCode::Esc);
        d.choose("Writing tools");
        let rows = d.app.menu_rows();
        for (label, action) in rows {
            assert!(
                d.shows(&format!("▸ {label}")),
                "selected {label} at {width}x{height}"
            );
            let (says, topic) = crate::app::overlays::about_row(&action, "part");
            let beginning = says
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                d.shows(&beginning),
                "{label} explanation at {width}x{height}"
            );
            assert!(d.shows("? more   esc back"), "footer at {width}x{height}");
            d.key(KeyCode::Char('?'));
            assert_eq!(help_topic(&d), Some(topic));
            d.key(KeyCode::Esc);
            d.key(KeyCode::Down);
        }
    }
}

#[test]
fn settings_and_back_again() {
    let mut d = Desk::open(book("settings", true), 120, 35);
    d.key(KeyCode::Esc);
    let at = d
        .app
        .menu()
        .iter()
        .position(|(_, a)| *a == palette::Action::Submenu(Sub::Settings))
        .unwrap();
    for _ in 0..at {
        d.key(KeyCode::Down);
    }
    d.key(KeyCode::Enter);
    assert!(matches!(
        d.app.overlay,
        Overlay::Sub {
            sub: Sub::Settings,
            ..
        }
    ));
    assert!(d.shows("Line width: 72 columns"));
    assert!(d.shows("typewriter scrolling"));
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::Menu { sel: at });
}

#[test]
fn a_short_terminal_scrolls_the_menu_to_quit() {
    let mut d = Desk::open(book("short", true), 80, 16);
    d.key(KeyCode::Esc);
    assert!(d.shows("Find anything"));
    assert!(d.shows("esc close"), "the key hints stay in view");
    d.key(KeyCode::Up);
    assert!(d.shows("▸ Quit Grimoire"));
}

#[test]
fn the_palette_runs_the_same_actions() {
    let mut d = Desk::open(book("palette", true), 120, 35);
    d.ctrl('k');
    assert!(matches!(d.app.overlay, Overlay::Palette { .. }));
    d.typed("focus mode");
    d.key(KeyCode::Enter);
    assert!(d.app.focus_mode);
}

// ---- the status bar ------------------------------------------------------

#[test]
fn at_80_columns_the_status_bar_leads_with_esc() {
    let mut d = Desk::open(book("status80", true), 80, 24);
    assert!(d.status().contains("Esc menu"), "tree: {}", d.status());
    d.key(KeyCode::Tab);
    assert!(d.status().contains("Esc menu"), "editor: {}", d.status());
}

// ---- notes beside the scene ------------------------------------------------

fn with_wren(tag: &str) -> Desk {
    let root = book(tag, true);
    fs::write(
        root.join("characters/wren.md"),
        "---\ntitle: Wren\n---\nA courier.\n",
    )
    .unwrap();
    let mut d = Desk::open(root, 120, 35);
    d.key(KeyCode::Tab);
    d.typed("Wren");
    d.key(KeyCode::Left);
    d.ctrl('o');
    assert!(d.app.codex.is_some(), "Ctrl-O on a name opens its note");
    assert!(d.shows("A courier."));
    d
}

#[test]
fn esc_closes_an_open_note_from_the_editor() {
    let mut d = with_wren("note-esc");
    assert_eq!(d.app.focus, Focus::Editor);
    assert!(d.status().contains("Esc close note"), "{}", d.status());
    d.key(KeyCode::Esc);
    assert!(d.app.codex.is_none());
    assert!(!d.menu_open(), "the first Esc only closes the note");
    d.key(KeyCode::Esc);
    assert!(d.menu_open());
}

#[test]
fn ctrl_o_again_puts_the_note_away() {
    let mut d = with_wren("note-toggle");
    d.ctrl('o');
    assert!(d.app.codex.is_none());
}

// ---- focus mode ------------------------------------------------------------

#[test]
fn focus_mode_hides_the_panes_and_comes_back() {
    let mut d = Desk::open(book("focus", true), 120, 35);
    assert!(d.shows("Manuscript"), "the tree is up");
    d.key(KeyCode::Tab);
    d.ctrl('d');
    assert!(d.app.focus_mode);
    assert!(!d.shows("Manuscript"), "the tree is hidden");
    assert!(!d.shows("clearing"), "so is the clearing");
    // Tab can't reach what isn't there.
    d.key(KeyCode::Tab);
    assert_eq!(d.app.focus, Focus::Editor);
    // Esc still opens the menu, and closing it goes back to focus mode.
    d.key(KeyCode::Esc);
    assert!(d.menu_open());
    d.key(KeyCode::Esc);
    assert!(d.app.focus_mode);
    d.ctrl('d');
    assert!(!d.app.focus_mode);
    assert!(d.shows("Manuscript"));
}

// ---- a new book --------------------------------------------------------------

#[test]
fn a_book_never_opened_starts_on_its_guide() {
    let d = Desk::open(book("guide", false), 120, 35);
    let open = d.app.open.expect("something is open");
    assert_eq!(d.app.project.nodes[open].area, project::Area::Format);
    assert!(d.status().contains("a new book"), "{}", d.status());
}

#[test]
fn a_book_opened_before_starts_on_its_first_scene() {
    let d = Desk::open(book("no-guide", true), 120, 35);
    let open = d.app.open.expect("something is open");
    assert_eq!(d.app.project.nodes[open].title, "Scene One");
}

#[test]
fn an_empty_scene_says_how_to_start() {
    let d = Desk::open(book("cue", true), 120, 35);
    assert!(d.shows("Tab or click to write"), "cue on an empty scene");
}

// ---- notes in the prose ----------------------------------------------------

#[test]
fn notes_and_tks_are_left_out_of_the_count() {
    let root = book("notes-count", true);
    let scene = first_scene(&root);
    let text = fs::read_to_string(&scene).unwrap();
    fs::write(
        &scene,
        format!("{text}One two %% three four %% five TK six.\n"),
    )
    .unwrap();
    let d = Desk::open(root, 120, 35);
    assert_eq!(d.app.project.total_words(), 4);
}

// ---- recovered words ---------------------------------------------------------

#[test]
fn letters_never_decide_what_happens_to_recovered_words() {
    let root = book("recover", true);
    let scene = first_scene(&root);
    recovery::keep(
        &root,
        &scene,
        "---\ntitle: Scene One\n---\nWords from before.\n",
    )
    .unwrap();
    let mut d = Desk::open(root.clone(), 120, 35);
    assert!(matches!(d.app.overlay, Overlay::Recover { .. }));
    for c in ['n', 'N', 'y', 'q', 'x'] {
        d.key(KeyCode::Char(c));
        assert!(
            matches!(d.app.overlay, Overlay::Recover { .. }),
            "{c} must not decide"
        );
    }
    let project = Project::load(&root).unwrap();
    assert_eq!(recovery::pending(&project).len(), 1, "still recoverable");
    // Enter on the first choice restores them.
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::None);
    let saved = fs::read_to_string(&scene).unwrap();
    assert!(saved.contains("Words from before."), "{saved}");
}

#[test]
fn deciding_later_keeps_the_recovered_words() {
    let root = book("recover-later", true);
    let scene = first_scene(&root);
    recovery::keep(&root, &scene, "---\ntitle: Scene One\n---\nLater words.\n").unwrap();
    let mut d = Desk::open(root.clone(), 120, 35);
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None);
    let project = Project::load(&root).unwrap();
    assert_eq!(recovery::pending(&project).len(), 1);
}

// ---- lists ------------------------------------------------------------------

#[test]
fn moving_through_themes_previews_each_and_esc_puts_yours_back() {
    let mut d = Desk::open(book("themes", true), 120, 35);
    let mine = d.app.theme.name.clone();
    d.key(KeyCode::F(9));
    assert!(matches!(d.app.overlay, Overlay::Themes { .. }));
    d.key(KeyCode::Down);
    assert_ne!(d.app.theme.name, mine, "the next theme shows at once");
    d.key(KeyCode::Char('k'));
    assert_eq!(d.app.theme.name, mine);
    d.key(KeyCode::Up);
    assert_ne!(d.app.theme.name, mine, "and it goes round the list");
    d.key(KeyCode::Esc);
    assert_eq!(d.app.theme.name, mine);
}

// ---- themes ---------------------------------------------------------------

fn fg_colours_in_row(d: &Desk, y: u16) -> Vec<ratatui::style::Color> {
    let buf = d.term.backend().buffer();
    let mut v: Vec<ratatui::style::Color> = (0..buf.area.width).map(|x| buf[(x, y)].fg).collect();
    v.sort_by_key(|c| format!("{c:?}"));
    v.dedup();
    v
}

#[test]
fn the_rainbow_theme_paints_its_accent_as_a_spectrum() {
    let mut d = Desk::open(book("rainbow", true), 120, 35);
    d.app.theme = theme::presets()
        .into_iter()
        .find(|t| t.is_rainbow())
        .unwrap();
    d.draw();
    let accent = d.app.theme.accent;
    let buf = d.term.backend().buffer();
    let flat = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| buf[(x, y)].fg == accent || buf[(x, y)].bg == accent)
        .count();
    assert_eq!(flat, 0, "no cell is left in the flat accent");
    // The focused tree's top border runs through several hues.
    assert!(
        fg_colours_in_row(&d, 0).len() >= 5,
        "the border is one colour"
    );
}

#[test]
fn a_flat_theme_keeps_its_accent() {
    let d = Desk::open(book("flat-accent", true), 120, 35);
    let accent = d.app.theme.accent;
    let buf = d.term.backend().buffer();
    assert_eq!(buf[(0, 0)].fg, accent, "the focused border is the accent");
}

#[test]
fn the_theme_picker_scrolls_on_a_short_terminal() {
    let mut d = Desk::open(book("picker", true), 80, 24);
    d.key(KeyCode::F(9));
    assert!(matches!(d.app.overlay, Overlay::Themes { .. }));
    assert!(d.shows("Grimoire"), "starts at the top");
    assert!(d.shows("↓ more"));
    // Up from the first wraps to the last: Custom…, with Rainbow above it.
    d.key(KeyCode::Up);
    assert!(d.shows("Custom…"), "the last row scrolled into view");
    assert!(d.shows("Rainbow"));
    assert!(d.shows("↑ more"));
    assert_eq!(d.app.theme.name, "Custom", "previewing the custom slot");
    d.key(KeyCode::Up);
    assert_eq!(d.app.theme.name, "Rainbow", "previewed as you move");
    d.key(KeyCode::Esc);
    assert_eq!(d.app.theme.name, "Grimoire", "Esc puts the old theme back");
}

// ---- the book at the top of the tree, and where focus mode is ----------------

#[test]
fn a_tall_tree_is_headed_by_the_spellbook_and_clicks_still_land() {
    let mut d = Desk::open(book("art-tall", true), 120, 42);
    assert!(d.shows("`───────────┴───────────'"), "the book is drawn");
    let rows = d.rows();
    let art = rows.iter().position(|r| r.contains("┴")).unwrap();
    let novel = rows
        .iter()
        .position(|r| r.contains("Novel Format"))
        .unwrap();
    assert!(novel > art, "the tree starts below the book");
    // Clicking Scene Two's row opens Scene Two, not whatever is a book's
    // height further down the list.
    let left = |r: &String| r.chars().take(30).collect::<String>();
    let y = rows
        .iter()
        .position(|r| left(r).contains("Scene Two"))
        .unwrap() as u16;
    let x = rows[y as usize].chars().position(|c| c == 'S').unwrap() as u16;
    d.app.on_click(x, y);
    d.draw();
    let open = d.app.open.map(|i| d.app.project.nodes[i].title.clone());
    assert_eq!(open.as_deref(), Some("Scene Two"));
}

#[test]
fn a_short_tree_keeps_its_rows_and_skips_the_book() {
    let d = Desk::open(book("art-short", true), 80, 24);
    assert!(!d.shows("┴───"), "no book on a short terminal");
    assert!(
        d.rows()[1].contains("Novel Format"),
        "the tree starts at the top"
    );
}

#[test]
fn focus_mode_is_named_where_it_can_be_used() {
    let mut d = Desk::open(book("focus-hint", true), 140, 42);
    assert!(
        d.shows("F2 timer · ^D focus mode"),
        "the idle timer says where focus mode is"
    );
    d.key(KeyCode::Tab);
    assert_eq!(d.app.focus, Focus::Editor);
    assert!(
        d.status().contains("Esc menu  ^D focus mode"),
        "{}",
        d.status()
    );
    // Running, the timer is "writing", so "focus" only ever means focus mode.
    d.key(KeyCode::F(2));
    assert!(d.shows("writing · "), "timer label while running");
    assert!(!d.rows().iter().any(|r| r.contains("┌ focus ·")));
}

#[test]
fn the_music_pane_says_its_keys_when_focused() {
    let root = book("music-keys", true);
    let setup = Setup {
        // On but unconfigured: the pane shows, nothing is polled.
        music: music::Config {
            enabled: true,
            ..music::Config::default()
        },
        theme: theme::default_theme(),
        settings: Settings::default(),
        background: false,
    };
    let app = App::with(Project::load(&root).unwrap(), setup).unwrap();
    let mut d = Desk {
        app,
        term: Terminal::new(TestBackend::new(140, 42)).unwrap(),
        armed: false,
        left: false,
        root,
    };
    d.draw();
    assert!(!d.shows("[ prev · space ⏯ · ] next"), "only while focused");
    for _ in 0..5 {
        if d.app.focus == Focus::Music {
            break;
        }
        d.key(KeyCode::Tab);
    }
    assert_eq!(d.app.focus, Focus::Music);
    assert!(
        d.shows("[ prev · space ⏯ · ] next"),
        "the pane's edge names the keys"
    );
    assert!(d.status().contains("[ ] track"), "{}", d.status());
}

// ---- spelling: click a misspelt word ----------------------------------------

/// A book whose first scene reads `body`, open on it with the dictionary
/// loaded (tests start no background work, so it's built here).
fn spelling_desk(tag: &str, body: &str) -> Desk {
    let root = book(tag, true);
    let scene = first_scene(&root);
    let text = fs::read_to_string(&scene).unwrap();
    let front_end = text.find("\n---\n").map(|i| i + 5).unwrap_or(0);
    fs::write(&scene, format!("{}\n{body}\n", &text[..front_end])).unwrap();
    let mut d = Desk::open(root, 120, 35);
    d.app.load_speller_now();
    d.draw();
    d
}

/// Where `needle` is on screen, (x, y), searching right of the tree.
fn at(d: &Desk, needle: &str) -> (u16, u16) {
    for (y, r) in d.rows().iter().enumerate() {
        let chars: Vec<char> = r.chars().collect();
        let n: Vec<char> = needle.chars().collect();
        for x in 31..chars.len().saturating_sub(n.len()) {
            if chars[x..x + n.len()] == n[..] {
                return (x as u16, y as u16);
            }
        }
    }
    panic!("{needle:?} not on screen:\n{}", d.rows().join("\n"));
}

fn click(d: &mut Desk, needle: &str) {
    let (x, y) = at(d, needle);
    d.app.on_click(x + 1, y);
    d.draw();
}

fn popup_open(d: &Desk) -> bool {
    matches!(d.app.overlay, Overlay::Spelling { inline: true, .. })
}

fn scene_text(d: &Desk) -> String {
    d.app.editor.text()
}

#[test]
fn clicking_a_misspelt_word_offers_its_fixes_and_one_undo_takes_one_back() {
    let mut d = spelling_desk("spell-fix", "Teh cat sat.");
    click(&mut d, "Teh cat");
    assert!(popup_open(&d), "the popup opens on the word");
    assert!(d.shows("“Teh”"));
    assert!(d.shows("Always ignore — every book"));
    // "The", in the word's own capitalisation, is on offer; clicking it fixes.
    click(&mut d, "The ");
    assert!(!popup_open(&d));
    assert!(
        scene_text(&d).contains("The cat sat."),
        "{}",
        scene_text(&d)
    );
    let i = d.app.open.unwrap();
    assert!(d.app.project.nodes[i].dirty, "autosave sees the change");
    d.ctrl('z');
    assert!(
        scene_text(&d).contains("Teh cat sat."),
        "one undo takes it back"
    );
}

#[test]
fn right_click_opens_the_same_popup() {
    let mut d = spelling_desk("spell-right", "Teh cat sat.");
    let (x, y) = at(&d, "Teh cat");
    d.app.on_right_click(x + 1, y);
    d.draw();
    assert!(popup_open(&d));
}

#[test]
fn always_ignore_in_this_book_writes_its_list_and_lifts_the_underline() {
    let mut d = spelling_desk("spell-book", "Qwyrk waits.");
    let (x, y) = at(&d, "Qwyrk");
    assert!(
        d.term.backend().buffer()[(x, y)]
            .modifier
            .contains(Modifier::UNDERLINED),
        "underlined to begin with"
    );
    click(&mut d, "Qwyrk");
    click(&mut d, "Always ignore — this book");
    let list = fs::read_to_string(d.root.join("dictionary.txt")).unwrap_or_default();
    assert!(list.lines().any(|l| l == "Qwyrk"), "{list}");
    d.key(KeyCode::End); // off the word, so it would be underlined again
    let (x, y) = at(&d, "Qwyrk");
    assert!(
        !d.term.backend().buffer()[(x, y)]
            .modifier
            .contains(Modifier::UNDERLINED),
        "no longer underlined"
    );
}

#[test]
fn the_every_book_list_is_kept_and_read_by_the_next_launch() {
    let dir = std::env::temp_dir().join(format!("grimoire-ui-userdict-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let list = dir.join("dictionary.txt");
    let mut d = spelling_desk("spell-user", "Zorblat waits.");
    d.app.user_dict = Some(list.clone());
    click(&mut d, "Zorblat");
    click(&mut d, "Always ignore — every book");
    assert!(spell::words_in(&list).contains(&"Zorblat".to_string()));
    // A fresh app — another book, even — accepts it once it reads the list.
    let mut other = spelling_desk("spell-user-2", "Zorblat returns.");
    let line = other
        .app
        .editor
        .lines
        .iter()
        .position(|l| l.contains("Zorblat"))
        .unwrap();
    assert!(
        !other.app.misspellings(line).is_empty(),
        "without the list it's a misspelling"
    );
    other.app.user_dict = Some(list.clone());
    other.app.load_speller_now();
    assert!(
        other.app.misspellings(line).is_empty(),
        "accepted in every book"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_letter_closes_the_popup_and_is_typed() {
    let mut d = spelling_desk("spell-type", "Teh cat sat.");
    click(&mut d, "Teh cat");
    assert!(popup_open(&d));
    d.key(KeyCode::Char('x'));
    assert!(!popup_open(&d), "typing puts the popup away");
    assert!(
        scene_text(&d).contains('x'),
        "and the letter lands in the prose"
    );
}

#[test]
fn clicking_a_correct_word_or_a_note_opens_nothing() {
    let mut d = spelling_desk("spell-none", "%% Teh %% the cat sat.");
    click(&mut d, "cat sat");
    assert!(!popup_open(&d), "a correct word just places the caret");
    click(&mut d, "Teh %%");
    assert!(!popup_open(&d), "notes aren't prose to correct");
}

#[test]
fn f8_offers_the_same_ways_to_ignore() {
    let mut d = spelling_desk("spell-f8", "Teh cat sat.");
    d.key(KeyCode::Tab);
    d.key(KeyCode::F(8));
    assert!(matches!(
        d.app.overlay,
        Overlay::Spelling { inline: false, .. }
    ));
    assert!(d.shows("Ignore for now"));
    assert!(d.shows("Always ignore — this book"));
    assert!(d.shows("Always ignore — every book"));
}

/// Not a test: `GRIMOIRE_SCENE_DUMP=<file> cargo test dump_pomodoro -- --ignored`
/// writes every theme's Pomodoro (idle, midday, night) as JSON cells with
/// their colours, for rendering a contact sheet to look at.
#[test]
#[ignore]
fn dump_pomodoro_previews() {
    let Ok(out) = std::env::var("GRIMOIRE_SCENE_DUMP") else {
        return;
    };
    let frame: u64 = std::env::var("GRIMOIRE_SCENE_FRAME")
        .ok()
        .and_then(|f| f.parse().ok())
        .unwrap_or(0);
    let hex = |c: ratatui::style::Color| match c {
        ratatui::style::Color::Rgb(r, g, b) => format!("{r:02x}{g:02x}{b:02x}"),
        _ => "ffffff".into(),
    };
    let mut themes = Vec::new();
    for t in theme::presets() {
        let mut shots = Vec::new();
        for (label, phase, p) in [
            ("idle", scene::Phase::Idle, 0.0),
            ("writing", scene::Phase::Focus, 0.42),
            ("break", scene::Phase::Break, 0.55),
        ] {
            let g = scene::render(scene::Scenery::for_theme(&t.name), phase, p, false, frame);
            let night = phase == scene::Phase::Break;
            let rows: Vec<Vec<(String, String)>> = g
                .iter()
                .enumerate()
                .map(|(y, row)| {
                    row.iter()
                        .enumerate()
                        .map(|(x, &(ch, ink))| {
                            let mut c = ui::scene_colour(ink, &t, night);
                            if t.is_rainbow() && c == t.accent {
                                c = theme::hue(x as f32 * 3.0 + y as f32 * 7.0);
                            }
                            (ch.to_string(), hex(c))
                        })
                        .collect()
                })
                .collect();
            shots.push(serde_json::json!({ "label": label, "rows": rows }));
        }
        themes.push(serde_json::json!({ "name": t.name, "border": hex(t.border), "shots": shots }));
    }
    fs::write(out, serde_json::to_string(&themes).unwrap()).unwrap();
}

// ---- the Pomodoro and the Visualizer ---------------------------------------

#[test]
fn every_theme_draws_its_own_pomodoro_through_the_real_desk() {
    let mut d = Desk::open(book("pomodoro-worlds", true), 140, 42);
    let mut seen: Vec<Vec<String>> = Vec::new();
    for t in theme::presets() {
        d.app.theme = t.clone();
        d.app.pomo.reset();
        d.draw();
        let pane = |d: &Desk| -> Vec<String> {
            let r = d.app.rect_scene;
            let rows = d.rows();
            (r.y..r.y + r.height)
                .map(|y| {
                    rows[y as usize]
                        .chars()
                        .skip(r.x as usize)
                        .take(r.width as usize)
                        .collect()
                })
                .collect()
        };
        let idle = pane(&d);
        assert!(
            !seen.contains(&idle),
            "{} looks like another theme's world",
            t.name
        );
        seen.push(idle);
        // Running, and on to night, across a spread of frames: no panics, and
        // the title keeps its words.
        d.key(KeyCode::F(2));
        for f in (0..300).step_by(37) {
            d.app.frame = f;
            d.draw();
        }
        assert!(d.shows("writing · "), "{}", t.name);
        d.app.pomo.phase = scene::Phase::Break;
        for f in (0..300).step_by(41) {
            d.app.frame = f;
            d.draw();
        }
        assert!(d.shows("break · "), "{}", t.name);
    }
}

#[test]
fn the_pane_switches_between_pomodoro_and_visualizer_and_there_is_no_garden() {
    let mut d = Desk::open(book("two-views", true), 140, 42);
    assert!(
        d.shows("pomodoro visualizer"),
        "the switcher names both views"
    );
    assert_eq!(d.app.pane_mode, scene::Mode::Pomodoro);
    for _ in 0..6 {
        if d.app.focus == Focus::Clearing {
            break;
        }
        d.key(KeyCode::Tab);
    }
    assert_eq!(d.app.focus, Focus::Clearing);
    d.key(KeyCode::Right);
    assert_eq!(d.app.pane_mode, scene::Mode::Visualizer);
    d.key(KeyCode::Right);
    assert_eq!(
        d.app.pane_mode,
        scene::Mode::Pomodoro,
        "two views, round and back"
    );
    d.key(KeyCode::Left);
    assert_eq!(d.app.pane_mode, scene::Mode::Visualizer);
    for word in ["garden", "clearing", "spectrum"] {
        assert!(!d.shows(word), "{word} is still on screen");
    }
}

// ---- the Visualizer names the song -------------------------------------------

/// The left column's text (the tree, then the timer/visualizer pane).
fn left_column(d: &Desk) -> Vec<String> {
    d.rows()
        .iter()
        .map(|r| r.chars().take(crate::ui::LEFT_W as usize).collect())
        .collect()
}

fn playing_a_long_song(d: &mut Desk) {
    d.app.music.state = crate::music::State::Playing(crate::music::Track {
        title: "An Extraordinarily Long Song Title That Keeps On Going".into(),
        artist: "Somebody With A Long Name".into(),
        progress: 40.0,
        duration: 200.0,
        playing: true,
    });
    d.app.pane_mode = crate::scene::Mode::Visualizer;
    d.draw();
}

#[test]
fn the_visualizer_names_the_song_truncated_to_the_pane() {
    // Grimoire writes the song along the bottom; Nord puts it in the title.
    for name in ["Grimoire", "Nord"] {
        let mut d = Desk::open(book(&format!("viz-song-{}", name.len()), true), 120, 42);
        d.app.theme = theme::presets()
            .into_iter()
            .find(|t| t.name == name)
            .unwrap();
        playing_a_long_song(&mut d);
        let col = left_column(&d);
        let row = col
            .iter()
            .find(|r| r.contains("♪ An Extraordinarily"))
            .unwrap_or_else(|| panic!("{name}: the song isn't named:\n{}", col.join("\n")));
        assert!(row.contains('…'), "{name}: cut with an ellipsis: {row}");
        assert!(
            !row.contains("Keeps On Going"),
            "{name}: and cut to fit: {row}"
        );
    }
}

// ---- a custom theme borrows a preset's Pomodoro and Visualizer ------------

/// The Pomodoro/Visualizer pane's glyphs, row by row.
fn pane_glyphs(d: &Desk) -> Vec<String> {
    let r = d.app.rect_scene;
    let buf = d.term.backend().buffer();
    (r.y..r.y + r.height)
        .map(|y| (r.x..r.x + r.width).map(|x| buf[(x, y)].symbol()).collect())
        .collect()
}

/// The pane as a preset draws it in `mode`, for comparing against.
fn preset_pane(name: &str, mode: scene::Mode) -> Vec<String> {
    let mut d = Desk::open(book(&format!("mix-ref-{}", name.len()), true), 120, 35);
    d.app.theme = theme::presets()
        .into_iter()
        .find(|t| t.name == name)
        .unwrap();
    d.app.pane_mode = mode;
    d.draw();
    pane_glyphs(&d)
}

#[test]
fn a_custom_theme_mixes_one_presets_world_with_anothers_visualizer() {
    let mut d = Desk::open(book("mix", true), 120, 35);
    // F9, up from the top wraps to Custom…, Enter edits it.
    d.key(KeyCode::F(9));
    d.key(KeyCode::Up);
    d.key(KeyCode::Enter);
    assert!(matches!(d.app.overlay, Overlay::Custom { .. }));
    assert!(
        d.shows("pomodoro") && d.shows("visualizer"),
        "the pick rows are listed"
    );
    // Down past the twelve swatches to the Pomodoro row: the pane shows it.
    for _ in 0..12 {
        d.key(KeyCode::Down);
    }
    assert_eq!(d.app.pane_mode, scene::Mode::Pomodoro);
    for _ in 0..20 {
        if d.app.theme.world() == "Kanagawa" {
            break;
        }
        d.key(KeyCode::Right);
    }
    assert_eq!(d.app.theme.world(), "Kanagawa");
    assert!(d.shows("◂ Kanagawa ▸"));
    assert_eq!(
        pane_glyphs(&d),
        preset_pane("Kanagawa", scene::Mode::Pomodoro),
        "the pane previews Kanagawa's world"
    );
    // The Visualizer row: the pane switches to it, and ← goes back round.
    d.key(KeyCode::Down);
    assert_eq!(d.app.pane_mode, scene::Mode::Visualizer);
    for _ in 0..25 {
        if d.app.theme.look() == "Synthwave '84" {
            break;
        }
        d.key(KeyCode::Left);
    }
    assert_eq!(d.app.theme.look(), "Synthwave '84");
    assert_eq!(
        pane_glyphs(&d),
        preset_pane("Synthwave '84", scene::Mode::Visualizer),
        "the pane previews Synthwave's visualizer"
    );
    // Hex keys do nothing on a pick row; the colours are untouched.
    let swatches = |t: &theme::Theme| {
        (0..theme::ROLES.len())
            .map(|i| t.role(i))
            .collect::<Vec<_>>()
    };
    let before = swatches(&d.app.theme);
    d.typed("abcdef");
    assert_eq!(swatches(&d.app.theme), before);
    assert_eq!(d.app.theme.name, "Custom");
}

// ---- a book in a synced folder -----------------------------------------------

#[test]
fn a_synced_book_with_history_inside_offers_to_move_it_out() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: no git");
        return;
    }
    let pid = std::process::id();
    let base = std::env::temp_dir().join(format!("grimoire-ui-synced-{pid}"));
    let _ = fs::remove_dir_all(&base);
    grimoire_core::paths::set_data_dir(base.join("data"));
    let root = base.join("Dropbox/Salt");
    fs::create_dir_all(&root).unwrap();
    project::scaffold(&root).unwrap();
    fs::create_dir_all(root.join(".grimoire")).unwrap();
    fs::write(root.join(".grimoire/progress.toml"), "").unwrap();
    let ok = std::process::Command::new("git")
        .args(["init", "-q", "."])
        .current_dir(&root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .is_ok_and(|s| s.success());
    assert!(ok && root.join(".git").is_dir());

    let mut d = Desk::open(root.clone(), 120, 40);
    assert_eq!(d.app.cloud, Some(grimoire_core::cloud::Client::Dropbox));
    assert!(d.app.history_in_book);
    // A real launch says so, once per book.
    d.app.msg.clear();
    d.app.note_where_it_syncs(true);
    assert!(
        d.app
            .msg
            .contains("writing history is inside this Dropbox folder"),
        "{}",
        d.app.msg
    );
    d.app.msg.clear();
    d.app.note_where_it_syncs(true);
    assert_eq!(
        d.app.msg, "in Dropbox — conflicts are caught and kept",
        "only once"
    );
    d.draw();
    d.key(KeyCode::Esc);
    d.choose("This book");
    assert!(d.shows("Move writing history out…"), "offered in the menu");
    d.choose("Move writing history out");
    assert!(!root.join(".git").exists(), "moved out: {}", d.status());
    assert!(!d.app.history_in_book);
    assert!(d.status().contains("moved out"), "{}", d.status());
    d.key(KeyCode::Esc);
    d.choose("This book");
    assert!(!d.shows("Move writing history out…"), "no longer offered");
    drop(d);
    let _ = fs::remove_dir_all(&base);
}

// ---- conflict copies: shown, named, settled, undone ----------------------

const DROPBOX_COPY: &str = "01-Scene-One (Josh's conflicted copy 2026-09-25).md";

/// A book whose first scene has a Dropbox conflict copy beside it.
fn conflicted(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = book(tag, true);
    let scene = first_scene(&root);
    fs::write(
        &scene,
        "---\ntitle: \"Scene One\"\n---\n\nThe ferry left at dawn.\n",
    )
    .unwrap();
    let copy = scene.with_file_name(DROPBOX_COPY);
    fs::write(
        &copy,
        "---\ntitle: \"Scene One\"\n---\n\nThe ferry left at noon, from the laptop.\n",
    )
    .unwrap();
    (root, scene, copy)
}

fn in_trash(root: &Path, needle: &str) -> bool {
    fs::read_dir(root.join(".grimoire/trash"))
        .map(|rd| {
            rd.flatten()
                .any(|e| fs::read_to_string(e.path()).is_ok_and(|t| t.contains(needle)))
        })
        .unwrap_or(false)
}

#[test]
fn a_sync_apps_copy_is_named_in_the_tree_and_the_status_bar_warns() {
    let (root, _, _) = conflicted("conflict-badge");
    let d = Desk::open(root, 140, 42);
    assert!(d.shows("Dropbox copy"), "the tree names whose copy it is");
    assert!(d.status().contains("⚠ 1 conflict"), "{}", d.status());
    // and it isn't counted: the book is only the scene's five words
    assert_eq!(d.app.project.total_words(), 5);
}

#[test]
fn settling_takes_the_copy_and_ctrl_z_puts_both_back() {
    let (root, scene, copy) = conflicted("conflict-take");
    let mut d = Desk::open(root.clone(), 140, 42);
    d.app.open_conflicts();
    d.draw();
    assert!(d.shows("SETTLE CONFLICTS"));
    assert!(d.shows("Scene One — Dropbox copy"));
    d.key(KeyCode::Enter);
    assert!(matches!(d.app.overlay, Overlay::Settle { .. }));
    assert!(d.shows("Take the Dropbox copy"));
    assert!(
        d.app.beside.as_ref().is_some_and(|b| b.path == copy),
        "the copy is beside the scene"
    );
    assert!(d.shows("at noon"), "and readable there");
    d.key(KeyCode::Enter); // take it
    assert!(
        fs::read_to_string(&scene)
            .unwrap()
            .contains("from the laptop")
    );
    assert!(!copy.exists() && in_trash(&root, "from the laptop"));
    assert!(!d.status().contains("conflict"), "{}", d.status());
    // Ctrl-Z, straight away from the editor: both versions as they were.
    d.ctrl('z');
    assert!(fs::read_to_string(&scene).unwrap().contains("left at dawn"));
    assert!(copy.exists(), "the copy is back beside it");
    assert!(d.status().contains("⚠ 1 conflict"));
}

#[test]
fn keeping_the_scene_sends_the_copy_to_the_trash_and_back() {
    let (root, scene, copy) = conflicted("conflict-keep");
    let mut d = Desk::open(root.clone(), 140, 42);
    d.app.open_conflicts();
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('2'));
    assert!(fs::read_to_string(&scene).unwrap().contains("left at dawn"));
    assert!(!copy.exists() && in_trash(&root, "from the laptop"));
    d.ctrl('z');
    assert!(copy.exists());
}

#[test]
fn keeping_both_makes_a_scene_after_it_that_counts() {
    let (root, scene, copy) = conflicted("conflict-both");
    let mut d = Desk::open(root.clone(), 140, 42);
    d.app.open_conflicts();
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('3'));
    let kept = scene.with_file_name("02-Scene-One-Dropbox-copy.md");
    assert!(kept.exists(), "right after the original");
    assert!(!copy.exists());
    let title = d
        .app
        .project
        .nodes
        .iter()
        .find(|n| n.path == kept)
        .map(|n| n.title.clone());
    assert_eq!(title.as_deref(), Some("Scene One — Dropbox copy"));
    assert_eq!(d.app.project.total_words(), 5 + 8, "and both count now");
    d.ctrl('z');
    assert!(copy.exists() && !kept.exists());
}

#[test]
fn deleting_a_scene_takes_its_copies_to_the_trash_and_undo_brings_all_back() {
    let (root, scene, copy) = conflicted("conflict-delete");
    let mut d = Desk::open(root.clone(), 140, 42);
    // The tree is on the scene; ask to delete it.
    let i = d
        .app
        .project
        .nodes
        .iter()
        .position(|n| n.path == scene)
        .unwrap();
    d.app.reveal(i);
    d.app.focus = Focus::Tree;
    d.key(KeyCode::Char('d'));
    assert!(
        d.shows("and its Dropbox copy"),
        "the prompt says the copy goes too"
    );
    d.key(KeyCode::Char('y'));
    assert!(!scene.exists() && !copy.exists());
    assert!(in_trash(&root, "left at dawn") && in_trash(&root, "from the laptop"));
    d.ctrl('z');
    assert!(scene.exists() && copy.exists(), "one undo brings both back");
}

// ---- files a sync client hasn't delivered -----------------------------------

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

/// An online-only scene with no connection: the book opens, the row says it
/// can't be read, and typing into it is refused with the reason — nothing is
/// ever written over it.
#[cfg(unix)]
#[test]
fn a_scene_that_cannot_be_read_opens_as_a_row_that_says_so() {
    let root = book("unreadable", true);
    let scene = first_scene(&root);
    let before = fs::read(&scene).unwrap();
    set_mode(&scene, 0o000);
    let mut d = Desk::open(root.clone(), 120, 35);
    assert!(
        d.status().contains("can't be read right now"),
        "{}",
        d.status()
    );
    assert!(d.shows("can't read"), "the row carries the reason");
    d.key(KeyCode::Tab);
    d.typed("x");
    assert!(d.status().contains("can't be edited yet"), "{}", d.status());
    assert_eq!(d.app.project.dirty_count(), 0);
    set_mode(&scene, 0o644);
    assert_eq!(fs::read(&scene).unwrap(), before, "never written");
}

/// A scene that becomes unreadable while it has unsaved words: the save
/// waits (it doesn't fail, and it doesn't write), the words are kept in
/// recovery, and the status bar says it's waiting.
#[cfg(unix)]
#[test]
fn unsaved_words_wait_for_a_file_that_cannot_be_read() {
    let root = book("waiting", true);
    let scene = first_scene(&root);
    let mut d = Desk::open(root.clone(), 120, 35);
    d.key(KeyCode::Tab);
    d.typed("Kept.");
    set_mode(&scene, 0o000);
    assert!(d.app.commit_saves(), "waiting isn't a failure");
    d.draw();
    assert!(d.status().contains("waiting"), "{}", d.status());
    assert!(d.app.msg.contains("can't be saved yet"), "{}", d.app.msg);
    assert_eq!(d.app.project.dirty_count(), 1, "still unsaved, not dropped");
    let rel = scene.strip_prefix(&root).unwrap();
    let kept = root.join(".grimoire/recovery").join(rel);
    assert!(
        fs::read_to_string(&kept).unwrap().contains("Kept."),
        "a copy in recovery"
    );
    set_mode(&scene, 0o644);
    assert!(
        !fs::read_to_string(&scene).unwrap().contains("Kept."),
        "not written while it couldn't be checked"
    );
    // Readable again: the next save goes through.
    d.app.project.nodes[d.app.open.unwrap()].disk.unavailable = None;
    assert!(d.app.commit_saves());
    assert!(fs::read_to_string(&scene).unwrap().contains("Kept."));
}

// ---- export: TKs first, the manuscript's look, the author's details ---------

/// A seen book with words in its first scene, the export dialog open.
fn export_desk(tag: &str, words: &str) -> Desk {
    let root = book(tag, true);
    let scene = first_scene(&root);
    let text = fs::read_to_string(&scene).unwrap();
    fs::write(&scene, format!("{text}\n{words}\n")).unwrap();
    let mut d = Desk::open(root, 120, 40);
    d.app.open_export();
    d.draw();
    d
}

fn export_rows(d: &Desk) -> (usize, usize, usize, usize) {
    let Overlay::Export { parts, .. } = &d.app.overlay else {
        panic!("the export dialog isn't up");
    };
    // Word, PDF, Paperback, EPUB, Markdown; then each part.
    let how_much = 5 + parts.len();
    (how_much, how_much + 1, how_much + 2, how_much + 3)
}

fn go_to(d: &mut Desk, row: usize) {
    for _ in 0..20 {
        if matches!(d.app.overlay, Overlay::Export { sel, .. } if sel == row) {
            return;
        }
        d.key(KeyCode::Down);
    }
    panic!("row {row} not reached");
}

#[test]
fn export_says_what_tks_are_left_before_it_writes_anything() {
    let mut d = export_desk("tk-warn", "She took the TK from her coat.");
    assert!(d.shows("PDF"), "PDF is on offer");
    d.typed("x");
    assert!(d.shows("1 TK is still in the text"), "warned first");
    assert!(d.shows("took the TK from her coat"));
    assert!(!d.root.join("exports").exists(), "nothing written yet");
    d.typed("x");
    assert!(
        (d.shows("✓ exports/") || d.shows("✓ exports\\")),
        "the second x exports anyway"
    );
    assert!(d.shows("1 TK is left in the text"));
    assert!(d.root.join("exports").exists());
}

#[test]
fn the_look_dialog_saves_to_novel_toml_and_goes_back_to_export() {
    let mut d = export_desk("look", "Words.");
    let (_, look, _, _) = export_rows(&d);
    go_to(&mut d, look);
    d.key(KeyCode::Enter);
    assert!(matches!(d.app.overlay, Overlay::Look { .. }));
    assert!(d.shows("MANUSCRIPT LOOK"));
    d.key(KeyCode::Right); // Format: Modern → Classic
    d.key(KeyCode::Down);
    d.key(KeyCode::Right); // Paper: Letter → A4
    d.key(KeyCode::Esc);
    assert!(
        matches!(d.app.overlay, Overlay::Export { .. }),
        "back to the export dialog"
    );
    let toml = fs::read_to_string(d.root.join("novel.toml")).unwrap();
    assert!(
        toml.contains("format = \"classic\"") && toml.contains("paper = \"a4\""),
        "{toml}"
    );
    assert!(toml.contains("part_label"), "the rest of novel.toml stays");
    assert!(d.shows("Classic (Courier) · A4"));
}

#[test]
fn author_details_are_typed_in_and_kept() {
    let mut d = export_desk("author", "Words.");
    let (_, _, author, _) = export_rows(&d);
    assert!(d.shows("not set — the title page wants your name"));
    go_to(&mut d, author);
    d.key(KeyCode::Enter);
    assert!(matches!(d.app.overlay, Overlay::Author { .. }));
    d.key(KeyCode::Down); // legal name
    d.typed("Jane Q Smith");
    for _ in 0..5 {
        d.key(KeyCode::Down); // to email
    }
    d.typed("jane@example.com");
    d.key(KeyCode::Esc);
    assert!(matches!(d.app.overlay, Overlay::Export { .. }));
    assert!(d.shows("Jane Q Smith · jane@example.com"));
    let toml = fs::read_to_string(d.root.join("novel.toml")).unwrap();
    assert!(toml.contains("legal_name = \"Jane Q Smith\""), "{toml}");
    assert!(toml.contains("email = \"jane@example.com\""));
    // A fresh desk reads them back.
    let p = Project::load(&d.root).unwrap();
    assert_eq!(p.meta.contact.legal_name, "Jane Q Smith");
}

#[test]
fn how_much_takes_a_number_and_names_the_sample() {
    let mut d = export_desk("sample", "Words.");
    let (how_much, ..) = export_rows(&d);
    go_to(&mut d, how_much);
    assert!(d.shows("◂ the whole book ▸"));
    d.typed("2");
    assert!(
        d.shows("◂ the first 2 chapters ▸"),
        "typing a number asks for chapters"
    );
    d.key(KeyCode::Right);
    assert!(d.shows("◂ chapters 1-3 ▸"));
    d.key(KeyCode::Right);
    assert!(d.shows("◂ the first 10000 words ▸"));
    d.typed("x");
    assert!(
        d.shows("_First-10000-Words.docx"),
        "the sample's own file name"
    );
}

// ---- the paperback in the export dialog ------------------------------------

#[test]
fn the_export_dialog_offers_a_paperback_and_its_trim() {
    let mut d = Desk::open(book("paperback-row", true), 120, 35);
    d.app.open_export();
    d.draw();
    assert!(d.shows("[ ] Paperback"), "offered, off by default");
    assert!(
        d.shows("◂ 6 × 9 in ▸"),
        "the trim from novel.toml's default"
    );
    d.key(KeyCode::Down);
    d.key(KeyCode::Down);
    assert!(d.status().contains("trim size") || d.shows("←→ trim size"));
    d.key(KeyCode::Right);
    assert!(d.shows("◂ 5 × 8 in ▸"), "→ changes the trim");
    assert!(d.shows("[x] Paperback"), "and ticks it");
    d.key(KeyCode::Left);
    d.key(KeyCode::Left);
    assert!(d.shows("◂ 5.5 × 8.5 in ▸"), "← goes back round");
}

#[test]
fn the_export_dialog_fits_a_short_terminal_and_scrolls_to_the_cursor() {
    // Three parts, a TK to warn about, 80×24: more rows than fit.
    let mut d = export_desk(
        "export-short",
        "She took the TK from her coat.\n\nThe TK was cold.\n\nHe said TK and left.",
    );
    d.term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    d.draw();
    assert!(d.shows("Word document"), "the top shows at first");
    assert!(d.shows("choose"), "the hint row is always there");
    d.typed("x"); // the TK warning adds rows
    assert!(d.shows("still in the text"));
    let Overlay::Export { parts, .. } = &d.app.overlay else {
        panic!("the export dialog isn't up");
    };
    let last = 5 + parts.len() + 3; // the Export button
    go_to(&mut d, last);
    assert!(d.shows("Export to exports/"), "the cursor's row is in view");
    assert!(d.shows("x export") || d.shows("esc close"), "and the hint");
    // Every row can be reached and seen, one at a time, going back up.
    for row in (0..last).rev() {
        d.key(KeyCode::Up);
        assert!(
            matches!(d.app.overlay, Overlay::Export { sel, .. } if sel == row),
            "row {row}"
        );
        assert!(
            d.rows().iter().any(|r| r.contains("▸")),
            "row {row}: the cursor is on screen"
        );
    }
}

#[test]
fn the_rainbow_flows_over_time() {
    let mut d = Desk::open(book("rainbow-flow", true), 120, 35);
    d.app.theme = theme::presets()
        .into_iter()
        .find(|t| t.is_rainbow())
        .unwrap();
    let corner = |d: &Desk| d.term.backend().buffer()[(0, 0)].fg;
    d.draw();
    let before = corner(&d);
    // A quarter of a cycle later the same cell is a different colour.
    std::thread::sleep(std::time::Duration::from_secs_f32(ui::RAINBOW_CYCLE / 4.0));
    d.draw();
    assert_ne!(corner(&d), before, "the border's colour moved");
    // And the phase always stays on the wheel.
    let p = ui::rainbow_phase();
    assert!((0.0..360.0).contains(&p));
}

// ---- help -------------------------------------------------------------------

/// The topic the help is open on, by id; None if it isn't open.
fn help_topic(d: &Desk) -> Option<&'static str> {
    match &d.app.overlay {
        Overlay::Help { sel, .. } => Some(crate::help::TOPICS[*sel].id),
        _ => None,
    }
}

fn reading(d: &Desk) -> bool {
    matches!(d.app.overlay, Overlay::Help { reading: true, .. })
}

#[test]
fn the_menu_has_a_help_row_that_opens_it() {
    let mut d = Desk::open(book("help-menu", true), 120, 35);
    d.key(KeyCode::Esc);
    // Just above Quit, at the bottom: Help & about, then Help first in it.
    for _ in 0..2 {
        d.key(KeyCode::Up); // round from the top: Quit, Help & about
    }
    assert!(d.shows("▸ Help & about"));
    d.key(KeyCode::Enter);
    assert!(d.shows("▸ Help…"), "the menu offers it");
    d.key(KeyCode::Enter);
    assert_eq!(
        help_topic(&d),
        Some("outline"),
        "the topic for the pane it was opened from"
    );
    assert!(d.shows("HELP"));
    // `?` from the menu itself, too.
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    assert!(d.menu_open());
    d.key(KeyCode::Char('?'));
    assert!(help_topic(&d).is_some());
}

#[test]
fn question_mark_in_the_outline_opens_the_help_on_the_outline() {
    let mut d = Desk::open(book("help-q", true), 120, 35);
    assert_eq!(d.app.focus, Focus::Tree);
    assert!(d.status().contains("? help"), "{}", d.status());
    d.key(KeyCode::Char('?'));
    assert_eq!(help_topic(&d), Some("outline"));
    assert!(d.shows("The outline"));
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None, "Esc closes it");
    assert_eq!(d.app.focus, Focus::Tree);
}

#[test]
fn question_mark_on_the_page_is_typed() {
    let mut d = Desk::open(book("help-type", true), 140, 42);
    d.key(KeyCode::Tab);
    // `?` can't be the help here, so the status bar names F1 where it fits.
    assert!(d.status().contains("F1 help"), "{}", d.status());
    d.typed("Who?");
    assert_eq!(d.app.overlay, Overlay::None);
    assert!(scene_text(&d).contains("Who?"), "{}", scene_text(&d));
}

#[test]
fn f1_opens_the_help_from_anywhere_and_closes_it_again() {
    let mut d = Desk::open(book("help-f1", true), 120, 35);
    d.key(KeyCode::F(1));
    assert_eq!(help_topic(&d), Some("outline"));
    d.key(KeyCode::F(1));
    assert_eq!(d.app.overlay, Overlay::None, "F1 again closes it");
    d.key(KeyCode::Tab);
    d.key(KeyCode::F(1));
    assert_eq!(help_topic(&d), Some("writing"));
    assert!(!d.menu_open(), "F1 isn't the menu any more");
    d.key(KeyCode::Esc);
    // Over a dialog: the help, then the dialog again.
    d.app.open_export();
    d.draw();
    d.key(KeyCode::F(1));
    assert_eq!(help_topic(&d), Some("compile"));
    assert!(d.shows("William Shunn"));
    d.key(KeyCode::Esc);
    assert!(
        matches!(d.app.overlay, Overlay::Export { .. }),
        "closing the help goes back to the dialog"
    );
}

/// A desk with music on (nothing polled) and the keys' commands caught.
fn music_desk(tag: &str) -> (Desk, std::sync::mpsc::Receiver<music::Cmd>) {
    music_desk_at(book(tag, true))
}

fn music_desk_at(root: PathBuf) -> (Desk, std::sync::mpsc::Receiver<music::Cmd>) {
    let setup = Setup {
        music: music::Config {
            enabled: true,
            ..music::Config::default()
        },
        theme: theme::default_theme(),
        settings: Settings::default(),
        background: false,
    };
    let mut app = App::with(Project::load(&root).unwrap(), setup).unwrap();
    let rx = app.music.catch_commands();
    let mut d = Desk {
        app,
        term: Terminal::new(TestBackend::new(140, 42)).unwrap(),
        armed: false,
        left: false,
        root,
    };
    d.draw();
    (d, rx)
}

fn focus_on(d: &mut Desk, pane: Focus) {
    for _ in 0..6 {
        if d.app.focus == pane {
            return;
        }
        d.key(KeyCode::Tab);
    }
    panic!("Tab never reached {pane:?}");
}

#[test]
fn each_place_opens_the_help_on_its_own_topic() {
    let open = |d: &mut Desk| {
        d.key(KeyCode::F(1));
        let id = help_topic(d);
        d.key(KeyCode::F(1));
        id
    };
    let (root, _, _) = conflicted("help-where");
    let mut d = Desk::open(root, 140, 42);
    assert_eq!(open(&mut d), Some("outline"));
    d.key(KeyCode::Tab);
    assert_eq!(open(&mut d), Some("writing"));
    d.ctrl('d');
    assert_eq!(open(&mut d), Some("focus"));
    d.ctrl('d');
    focus_on(&mut d, Focus::Clearing);
    assert!(d.status().contains("? help"), "{}", d.status());
    assert_eq!(open(&mut d), Some("pomodoro"));
    d.key(KeyCode::Right);
    assert_eq!(d.app.pane_mode, scene::Mode::Visualizer);
    d.key(KeyCode::Char('?'));
    assert_eq!(help_topic(&d), Some("visualizer"));
    d.key(KeyCode::Esc);
    d.key(KeyCode::Left);

    // A scene beside, and settling conflicts.
    d.app.open_beside_picker();
    d.draw();
    d.key(KeyCode::Enter);
    assert!(d.app.beside.is_some(), "{}", d.app.msg);
    focus_on(&mut d, Focus::Beside);
    assert_eq!(open(&mut d), Some("beside"));
    d.app.open_conflicts();
    d.draw();
    assert_eq!(open(&mut d), Some("sync"));

    let mut d = with_wren("help-note");
    focus_on(&mut d, Focus::Codex);
    d.key(KeyCode::Char('?'));
    assert_eq!(help_topic(&d), Some("notebook"));

    let (mut d, _rx) = music_desk("help-music");
    focus_on(&mut d, Focus::Music);
    assert!(d.status().contains("? help"), "{}", d.status());
    d.key(KeyCode::Char('?'));
    assert_eq!(help_topic(&d), Some("music"));
}

#[test]
fn typing_searches_every_topic_and_enter_opens_the_first() {
    let mut d = Desk::open(book("help-search", true), 120, 35);
    d.key(KeyCode::F(1));
    assert!(d.shows("The corkboard"), "every topic listed");
    d.typed("shunn");
    assert!(d.shows("search shunn"));
    assert!(d.shows("Compile: the manuscript"));
    assert!(!d.shows("The corkboard"), "topics that don't mention it go");
    assert_eq!(help_topic(&d), Some("compile"));
    // Backspace edits the search.
    d.key(KeyCode::Backspace);
    assert!(d.shows("search shun"));
    d.typed("n");
    d.key(KeyCode::Enter);
    assert!(reading(&d));
    assert!(d.shows("William Shunn"));
    // Esc clears the search first, then closes.
    d.key(KeyCode::Esc);
    assert!(matches!(&d.app.overlay, Overlay::Help { query, .. } if query.is_empty()));
    assert!(d.shows("The corkboard"));
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None);
    // Nothing found says so, and Enter does nothing.
    d.key(KeyCode::F(1));
    d.typed("zzqqxx");
    assert!(d.shows("no topic matches"));
    d.key(KeyCode::Enter);
    assert!(!reading(&d));
}

#[test]
fn find_anything_reaches_every_topic_but_actions_come_first() {
    let mut d = Desk::open(book("help-palette", true), 120, 35);
    let empty = palette::filter(&palette::entries(&d.app), "");
    assert!(
        empty.iter().any(|e| e.action == palette::Action::Help)
            && !empty
                .iter()
                .any(|e| matches!(e.action, palette::Action::HelpTopic(_))),
        "an empty search lists Help, not every topic"
    );
    d.ctrl('k');
    d.typed("sprint");
    d.key(KeyCode::Enter);
    assert!(
        matches!(d.app.overlay, Overlay::Sprint { .. }),
        "\"sprint\" starts one: {:?}",
        d.app.overlay
    );
    d.key(KeyCode::Esc);
    d.ctrl('k');
    d.typed("help sprint");
    d.key(KeyCode::Enter);
    assert_eq!(help_topic(&d), Some("sprints"));
    assert!(reading(&d), "a topic chosen by name opens to read");
    d.key(KeyCode::Esc);
    d.ctrl('k');
    d.typed("help");
    d.key(KeyCode::Enter);
    assert_eq!(
        help_topic(&d),
        Some("outline"),
        "Help on its own: where you are"
    );
}

/// The last line of a topic's words, as the article sets it.
fn last_words(id: &str, width: usize) -> String {
    let t = theme::default_theme();
    let body = crate::help::TOPICS[crate::help::index(id)].body;
    let lines = crate::app::overlays::help_lay_out(body, width, &t);
    let last = lines.iter().rev().find(|l| l.width() > 0).unwrap();
    last.spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<String>()
        .trim()
        .to_string()
}

#[test]
fn the_longest_article_pages_down_to_its_last_line() {
    let longest = crate::help::TOPICS
        .iter()
        .max_by_key(|t| t.body.len())
        .unwrap();
    let mut d = Desk::open(book("help-long", true), 80, 24);
    d.app.open_help(Some(longest.id));
    d.draw();
    assert!(reading(&d));
    // The article's column at 80 wide: the box, the list, the margins.
    let width = (80 - 4 - 2) - 30 - 4;
    let end = last_words(longest.id, width);
    assert!(!d.shows(&end), "{end:?} starts below the fold");
    assert!(d.shows("↓ more"));
    for _ in 0..200 {
        if !d.shows("↓ more") {
            break;
        }
        d.key(KeyCode::PageDown);
    }
    assert!(d.shows(&end), "PgDn reaches {end:?}");
    assert!(!d.shows("↓ more"));
    d.key(KeyCode::PageDown);
    assert!(d.shows(&end), "and stops there");
    d.key(KeyCode::Home);
    assert!(!d.shows(&end));
    // The wheel reads on too, wherever the pointer is.
    d.app.on_scroll(40, 10, true);
    d.draw();
    assert!(matches!(d.app.overlay, Overlay::Help { scroll: 3, .. }));
    d.app.on_scroll(40, 10, false);
    d.draw();
    assert!(matches!(d.app.overlay, Overlay::Help { scroll: 0, .. }));
    d.key(KeyCode::End);
    assert!(d.shows(&end));
    d.key(KeyCode::PageUp);
    assert!(!d.shows(&end), "PgUp comes back");
}

#[test]
fn every_topic_fits_and_scrolls_at_80x24_and_120x35() {
    // Set at any width, no line is wider than the column it's set in.
    let t = theme::default_theme();
    for topic in crate::help::TOPICS {
        for width in 12..=120 {
            for line in crate::app::overlays::help_lay_out(topic.body, width, &t) {
                assert!(line.width() <= width, "{} at {width}: {line:?}", topic.id);
            }
        }
    }
    // And every word is there, whole: lines break between words. (Sorted,
    // since a table's key and its words are side by side.)
    for topic in crate::help::TOPICS {
        let words = |text: &str| -> Vec<String> {
            text.split_whitespace()
                .filter(|w| *w != "•")
                .map(String::from)
                .collect()
        };
        let runs = |r: &[crate::help::Run]| r.iter().map(|r| r.text.as_str()).collect::<String>();
        let mut want = Vec::new();
        for b in crate::help::parse(topic.body) {
            use crate::help::Block as B;
            match b {
                B::Heading(_, r) | B::Paragraph(r) | B::Bullet(r) => want.extend(words(&runs(&r))),
                B::Row { head: true, .. } => {}
                B::Row { key, text, .. } => {
                    want.extend(words(&runs(&key)));
                    want.extend(words(&runs(&text)));
                }
            }
        }
        let mut set: Vec<String> = crate::app::overlays::help_lay_out(topic.body, 60, &t)
            .iter()
            .flat_map(|l| {
                words(
                    &l.spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect::<String>(),
                )
            })
            .collect();
        set.sort();
        want.sort();
        assert_eq!(set, want, "{}", topic.id);
    }
    // Drawn, top and bottom, side by side and one at a time, nothing breaks
    // and the box's right edge is never written over.
    for (w, h) in [(80, 24), (120, 35), (60, 20)] {
        let mut d = Desk::open(book(&format!("help-fit{w}"), true), w, h);
        for topic in crate::help::TOPICS {
            d.app.open_help(Some(topic.id));
            d.draw();
            let right = d.rows()[3].chars().rev().nth(2).unwrap();
            assert_eq!(right, '│', "{} at {w}x{h}", topic.id);
            d.key(KeyCode::End);
            assert!(
                !d.shows("↓ more"),
                "{} at {w}x{h}: End is the end",
                topic.id
            );
            let last = d.rows()[h as usize - 4].clone();
            assert!(
                last.trim_end().ends_with("│ │"),
                "{} at {w}x{h}: {last}",
                topic.id
            );
            d.key(KeyCode::F(1));
        }
    }
}

#[test]
fn every_palette_action_is_explained_in_the_help() {
    let words: String = crate::help::TOPICS
        .iter()
        .map(|t| format!("{}\n{}", t.name, t.body))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
        .replace(['`', '*'], "");
    // An entry's name without what it says about itself: "Delete (to the
    // trash)" is "delete", "Turn spellcheck off" is "spellcheck".
    let name = |label: &str| {
        let mut n = label.to_lowercase();
        for cut in [" (", " — ", ": "] {
            if let Some(i) = n.find(cut) {
                n.truncate(i);
            }
        }
        let n = n.trim_end_matches('…');
        let n = n.strip_prefix("turn ").unwrap_or(n);
        let n = n
            .strip_suffix(" off")
            .or(n.strip_suffix(" on"))
            .unwrap_or(n);
        n.to_string()
    };
    // Every row, with everything that changes a row's name both ways.
    let (root, _, _) = conflicted("help-coverage");
    let mut d = Desk::open(root, 120, 35);
    let mut missing = Vec::new();
    for on in [false, true] {
        d.app.focus_mode = on;
        d.app.echo_on = on;
        d.app.spell_on = on;
        d.app.icons_on = on;
        d.app.typewriter = on;
        d.app.history_in_book = on;
        d.app.music.enabled = on;
        for e in palette::entries(&d.app) {
            use palette::Action as A;
            if matches!(
                e.action,
                A::Open(_) | A::Theme(_) | A::Beside(_) | A::HelpTopic(_)
            ) {
                continue;
            }
            if !words.contains(&name(&e.label)) {
                missing.push(e.label);
            }
        }
    }
    assert!(missing.is_empty(), "the help never names: {missing:?}");
}

/// A key as "Every key" writes it — `Ctrl-K`, `Alt-↑`, `Shift-Tab`, `PgDn`,
/// `Space`, `F7`, `?` — as the terminal sends it.
fn key_named(name: &str) -> KeyEvent {
    let mut mods = KeyModifiers::NONE;
    let mut rest = name;
    loop {
        if let Some(r) = rest.strip_prefix("Ctrl-") {
            mods |= KeyModifiers::CONTROL;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("Alt-") {
            mods |= KeyModifiers::ALT;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("Shift-") {
            mods |= KeyModifiers::SHIFT;
            rest = r;
        } else {
            break;
        }
    }
    let code = match rest {
        "Esc" => KeyCode::Esc,
        "Enter" => KeyCode::Enter,
        "Tab" if mods.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
        "Tab" => KeyCode::Tab,
        "Space" => KeyCode::Char(' '),
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PgUp" => KeyCode::PageUp,
        "PgDn" => KeyCode::PageDown,
        "↑" => KeyCode::Up,
        "↓" => KeyCode::Down,
        "←" => KeyCode::Left,
        "→" => KeyCode::Right,
        f if f.len() > 1 && f.starts_with('F') => KeyCode::F(f[1..].parse().unwrap()),
        c if c.chars().count() == 1 => {
            let c = c.chars().next().unwrap();
            // Ctrl-K is sent as a small k.
            KeyCode::Char(if mods.is_empty() {
                c
            } else {
                c.to_ascii_lowercase()
            })
        }
        other => panic!("\"Every key\" has a key this test can't press: {other:?}"),
    };
    KeyEvent::new(code, mods)
}

/// Everything a key could have changed that a writer would notice.
fn noticed(d: &Desk) -> String {
    let e = &d.app.editor;
    format!(
        "{:?}|{:?}|{:?}|{}|{},{},{}|{}|{}|{:?}|{:?}|{:?}|{:?}",
        d.rows(),
        d.app.overlay,
        d.app.focus,
        d.app.msg,
        e.cy,
        e.cx,
        e.scroll,
        d.left,
        d.app.sel,
        d.app.pane_mode,
        d.app
            .project
            .nodes
            .iter()
            .map(|n| &n.path)
            .collect::<Vec<_>>(),
        d.app.codex,
        d.app.beside,
    )
}

/// Every key "Every key" lists, by its section: what to press first so that
/// pressing it has something to do. Kept by hand, and checked against the
/// help both ways, so neither can change without the other.
const EVERY_KEY: &[(&str, &str, &[&str])] = &[
    ("Everywhere", "Esc", &[]),
    ("Everywhere", "F1", &[]),
    ("Everywhere", "?", &[]),
    ("Everywhere", "Ctrl-K", &[]),
    ("Everywhere", "Tab", &[]),
    ("Everywhere", "Shift-Tab", &[]),
    ("Everywhere", "Ctrl-S", &[]),
    ("Everywhere", "Ctrl-Q", &[]),
    ("Everywhere", "Ctrl-Z", &["n", "Enter"]),
    ("Everywhere", "Ctrl-Y", &["n", "Enter", "Ctrl-Z"]),
    ("Everywhere", "Ctrl-F", &["Tab"]),
    ("Everywhere", "Ctrl-D", &[]),
    ("Everywhere", "Ctrl-T", &[]),
    ("Everywhere", "Ctrl-O", &["Tab"]),
    ("Everywhere", "Ctrl-V", &["Tab"]),
    ("Everywhere", "Ctrl-A", &["Tab"]),
    ("Everywhere", "Ctrl-B", &["Tab"]),
    ("Everywhere", "Ctrl-I", &["Tab"]),
    ("Everywhere", "Alt-↑", &["n", "Enter"]),
    ("Everywhere", "Alt-↓", &[]),
    ("Everywhere", "F2", &[]),
    ("Everywhere", "F3", &["F2"]),
    ("Everywhere", "F4", &[]),
    ("Everywhere", "F5", &[]),
    ("Everywhere", "F6", &[]),
    ("Everywhere", "F7", &[]),
    ("Everywhere", "F8", &[]),
    ("Everywhere", "F9", &[]),
    ("Everywhere", "F10", &[]),
    ("Everywhere", "F11", &[]),
    ("Everywhere", "F12", &[]),
    ("The outline", "↑", &[]),
    ("The outline", "↓", &["↑"]),
    ("The outline", "j", &["↑"]),
    ("The outline", "k", &[]),
    ("The outline", "→", &["↑", "←"]),
    ("The outline", "l", &["↑", "←"]),
    ("The outline", "←", &["↑"]),
    ("The outline", "h", &["↑"]),
    ("The outline", "Enter", &["↑"]),
    ("The outline", "Space", &["↑"]),
    ("The outline", "n", &[]),
    ("The outline", "c", &[]),
    ("The outline", "p", &[]),
    ("The outline", "N", &[]),
    ("The outline", "r", &[]),
    ("The outline", "d", &[]),
    ("The outline", "u", &[]),
    ("The outline", "i", &[]),
    ("The outline", "K", &["n", "Enter"]),
    ("The outline", "J", &[]),
    ("The outline", "H", &[]),
    ("The outline", "b", &[]),
    ("The outline", "v", &["↓"]),
    ("The outline", "/", &[]),
    ("The outline", "t", &[]),
    ("The outline", "q", &[]),
    ("The page", "Home", &[]),
    ("The page", "End", &["Home"]),
    ("The page", "PgUp", &[]),
    ("The page", "PgDn", &["PgUp"]),
    ("The page", "Ctrl-C", &[]),
    ("The page", "Ctrl-X", &[]),
    ("The page", "Ctrl-Shift-Z", &["x", "Ctrl-Z"]),
    ("The find bar", "Enter", &[]),
    ("The find bar", "↓", &[]),
    ("The find bar", "↑", &[]),
    ("The find bar", "Ctrl-R", &[]),
    ("The find bar", "Ctrl-W", &[]),
    ("The find bar", "Alt-W", &[]),
    ("The find bar", "Ctrl-E", &[]),
    ("The find bar", "Alt-C", &[]),
    ("The Pomodoro pane", "Enter", &[]),
    ("The Pomodoro pane", "Space", &[]),
    ("The Pomodoro pane", "r", &["Enter"]),
    ("The Pomodoro pane", "←", &[]),
    ("The Pomodoro pane", "→", &[]),
    ("The Pomodoro pane", "h", &[]),
    ("The Pomodoro pane", "l", &[]),
    ("The music pane", "[", &[]),
    ("The music pane", "]", &[]),
    ("The music pane", "←", &[]),
    ("The music pane", "→", &[]),
    ("The music pane", "Space", &[]),
    ("The music pane", "r", &[]),
    ("The music pane", "s", &[]),
    ("The music pane", "+", &[]),
    ("The music pane", "-", &[]),
    ("The music pane", "l", &[]),
    ("The music pane", "Enter", &[]),
    ("A note or scene beside", "↑", &["↓"]),
    ("A note or scene beside", "↓", &[]),
    ("A note or scene beside", "Enter", &[]),
    ("A note or scene beside", "o", &[]),
    ("A note or scene beside", "q", &[]),
    ("A note or scene beside", "Esc", &[]),
    ("Help", "↑", &[]),
    ("Help", "↓", &[]),
    ("Help", "j", &["Enter"]),
    ("Help", "k", &["Enter", "PgDn"]),
    ("Help", "Enter", &[]),
    ("Help", "→", &[]),
    ("Help", "Tab", &[]),
    ("Help", "PgDn", &[]),
    ("Help", "PgUp", &["PgDn"]),
    ("Help", "Space", &["Enter"]),
    ("Help", "Home", &["PgDn"]),
    ("Help", "End", &[]),
    ("Help", "←", &["Enter"]),
    ("Help", "h", &["Enter"]),
    ("Help", "F1", &[]),
];

/// A desk where one section's keys are live: the music caught, the page
/// long enough to page through, a name with a note in two scenes.
fn desk_for(section: &str) -> (Desk, std::sync::mpsc::Receiver<music::Cmd>) {
    let tag = format!("keys-{}", section.replace(' ', "-").to_lowercase());
    let root = book(&tag, true);
    if section == "A note or scene beside" {
        fs::write(
            root.join("characters/wren.md"),
            "---\ntitle: Wren\n---\nA courier.\n",
        )
        .unwrap();
        // Two scenes that name her, so the note lists both.
        let one = first_scene(&root);
        fs::write(&one, "---\ntitle: \"Scene One\"\n---\n\nWren waited.\n").unwrap();
        let two = one.with_file_name("02-Scene-Two.md");
        fs::write(&two, "---\ntitle: \"Scene Two\"\n---\n\nWren again.\n").unwrap();
    }
    let (mut d, rx) = music_desk_at(root);
    let long: String = (1..=80)
        .map(|n| format!("Wren walked on, line {n}.\n\n"))
        .collect();
    match section {
        "Everywhere" | "The outline" => {}
        "The page" => {
            d.key(KeyCode::Tab);
            for c in long.chars().take(2000) {
                d.app.editor.insert(c);
            }
            d.draw();
        }
        "The find bar" => {
            d.key(KeyCode::Tab);
            d.typed("the cat and the dog and the end");
            d.key(KeyCode::Home);
            d.ctrl('f');
            d.typed("the");
        }
        "The Pomodoro pane" => focus_on(&mut d, Focus::Clearing),
        "The music pane" => focus_on(&mut d, Focus::Music),
        "A note or scene beside" => {
            d.key(KeyCode::Tab);
            d.key(KeyCode::Right);
            d.ctrl('o');
            assert!(d.app.codex.is_some(), "the note opens");
            focus_on(&mut d, Focus::Codex);
        }
        "Help" => d.key(KeyCode::F(1)),
        other => panic!("no desk for the section {other:?}"),
    }
    (d, rx)
}

#[test]
fn every_key_in_every_key_does_something() {
    let body = crate::help::TOPICS[crate::help::index("keys")].body;
    let documented = crate::help::table_keys(body);
    let listed: Vec<(String, String)> = EVERY_KEY
        .iter()
        .map(|(s, k, _)| (s.to_string(), k.to_string()))
        .collect();
    for key in &documented {
        assert!(
            listed.contains(key),
            "\"Every key\" lists {key:?}; add it here"
        );
    }
    for key in &listed {
        assert!(
            documented.contains(key),
            "{key:?} is here but not in \"Every key\""
        );
    }
    let mut dead = Vec::new();
    for (section, key, first) in EVERY_KEY {
        let (mut d, rx) = desk_for(section);
        for k in *first {
            d.press(key_named(k));
        }
        d.app.msg.clear();
        d.draw();
        while rx.try_recv().is_ok() {}
        let before = noticed(&d);
        d.press(key_named(key));
        if noticed(&d) == before && rx.try_recv().is_err() {
            dead.push(format!("{section}: {key}"));
        }
    }
    assert!(dead.is_empty(), "these keys did nothing: {dead:#?}");
}

#[test]
fn about_says_the_version_and_who_made_it_and_links_to_the_studio() {
    let mut d = Desk::open(book("about", true), 120, 40);
    d.key(KeyCode::Esc);
    d.choose("Help & about");
    d.choose("About Grimoire");
    assert!(matches!(d.app.overlay, Overlay::About));
    assert!(d.shows(&format!("version {}", env!("CARGO_PKG_VERSION"))));
    assert!(d.shows("Catfinity Studios"));
    assert!(d.shows("catfinity.com"));
    // A click on the link, and Enter, both head for the studio's site.
    let r = d.app.about_link.get();
    d.app.on_click(r.x + r.width / 2, r.y);
    assert_eq!(d.app.last_opened.as_deref(), Some("https://catfinity.com"));
    d.app.last_opened = None;
    d.key(KeyCode::Enter);
    assert_eq!(d.app.last_opened.as_deref(), Some("https://catfinity.com"));
    d.key(KeyCode::Esc);
    assert!(matches!(d.app.overlay, Overlay::None));
}

#[test]
fn about_introduces_catfinity_and_links_to_it() {
    let mut d = Desk::open(book("about-game", true), 120, 40);
    d.app.open_about();
    d.draw();
    assert!(d.shows("makers of Catfinity, an idle cat RPG"));
    assert!(d.shows("play Catfinity on iPhone & Android"));
    let r = d.app.about_game_link.get();
    d.app.on_click(r.x + r.width / 2, r.y);
    assert_eq!(
        d.app.last_opened.as_deref(),
        Some("https://catfinity.com/catfinity/")
    );
    d.app.last_opened = None;
    d.key(KeyCode::Char('c'));
    assert_eq!(
        d.app.last_opened.as_deref(),
        Some("https://catfinity.com/catfinity/")
    );
}

#[test]
fn about_fits_a_short_terminal() {
    let mut d = Desk::open(book("about-short", true), 80, 24);
    d.app.open_about();
    d.draw();
    assert!(d.shows("Catfinity Studios"));
    assert!(d.shows("esc close"), "the hint row still shows");
}

#[test]
fn about_opens_the_license_which_says_commercial_use_is_fine_with_credit() {
    let mut d = Desk::open(book("license", true), 120, 40);
    d.app.open_about();
    d.draw();
    assert!(d.shows("free for commercial use"));
    d.key(KeyCode::Char('l'));
    assert!(matches!(d.app.overlay, Overlay::Help { .. }));
    assert!(d.shows("commercial work included"));
    assert!(d.shows("Credit is required"));
    // Closing the help brings About back.
    d.key(KeyCode::Esc);
    assert!(matches!(d.app.overlay, Overlay::About));
    // Ctrl-K gets there too.
    d.key(KeyCode::Esc);
    d.app.run_action(crate::palette::Action::License);
    d.draw();
    assert!(d.shows("Credit is required"));
}

#[test]
fn donate_says_it_is_never_required_and_links_to_kofi() {
    let mut d = Desk::open(book("donate", true), 120, 40);
    d.key(KeyCode::Esc);
    d.choose("Help & about");
    d.choose("Donate…");
    assert!(matches!(d.app.overlay, Overlay::Donate));
    assert!(d.shows("never"));
    assert!(d.shows("required"));
    assert!(d.shows("appreciated"));
    assert!(d.shows("ko-fi.com/F2F21E0DK0"));
    let r = d.app.about_link.get();
    d.app.on_click(r.x + r.width / 2, r.y);
    assert_eq!(
        d.app.last_opened.as_deref(),
        Some("https://ko-fi.com/F2F21E0DK0")
    );
    // From About, `d` gets here too.
    d.key(KeyCode::Esc);
    d.app.open_about();
    d.draw();
    d.key(KeyCode::Char('d'));
    assert!(matches!(d.app.overlay, Overlay::Donate));
}

// ---- more than one book --------------------------------------------------

#[test]
fn a_new_book_from_the_menu_saves_this_one_and_opens_the_new_one() {
    let first = book("book-first", true);
    let scene = first_scene(&first);
    let state = first.with_extension("state.toml");
    let name = format!("grimoire-ui-book-second-{}", std::process::id());
    let second = first.parent().unwrap().join(&name);
    let _ = fs::remove_dir_all(&second);
    let mut d = Desk::open(first.clone(), 120, 35);
    d.app.state_file = Some(state.clone());
    d.key(KeyCode::Tab);
    d.typed("Words from the first book");
    d.key(KeyCode::Esc);
    d.choose("Start a new book…");
    assert!(d.shows("START A NEW BOOK"));
    // Typing is typing here: ? and q are letters.
    d.typed("?q");
    assert!(matches!(d.app.overlay, Overlay::NewBook { .. }));
    d.key(KeyCode::Backspace);
    d.key(KeyCode::Backspace);
    d.typed(&name);
    assert!(d.shows("it'll live in"), "says where before it makes it");
    d.key(KeyCode::Enter);

    assert_eq!(d.app.project.root, fs::canonicalize(&second).unwrap());
    assert!(second.join("manuscript").is_dir());
    assert!(d.status().contains("is ready"), "{}", d.status());
    assert!(
        fs::read_to_string(&scene)
            .unwrap()
            .contains("Words from the first book"),
        "the first book was saved before it was put away"
    );
    let remembered = fs::read_to_string(&state).unwrap();
    assert!(remembered.starts_with(&format!("last = \"{}\"", d.app.project.root.display())));

    // And back again, from Open another book.
    d.key(KeyCode::Esc);
    d.choose("Open another book…");
    assert!(d.shows("OPEN ANOTHER BOOK"));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.project.root, fs::canonicalize(&first).unwrap());
    assert!(d.status().contains("opened"), "{}", d.status());
    let _ = fs::remove_dir_all(&second);
    let _ = fs::remove_file(&state);
}

#[test]
fn a_new_book_never_lands_in_a_folder_with_other_things_in_it() {
    let first = book("book-taken", true);
    let name = format!("grimoire-ui-taken-folder-{}", std::process::id());
    let taken = first.parent().unwrap().join(&name);
    fs::create_dir_all(&taken).unwrap();
    fs::write(taken.join("shopping.txt"), "eggs").unwrap();
    let mut d = Desk::open(first.clone(), 120, 35);
    d.app.run_action(palette::Action::NewBook);
    d.typed(&name);
    assert!(d.shows("has other things in it"), "{}", d.rows().join("\n"));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.project.root, first, "still the same book");
    assert!(!taken.join("manuscript").exists());
    let _ = fs::remove_dir_all(&taken);
}

#[test]
fn a_draft_brought_in_becomes_a_book_split_at_its_chapters() {
    let first = book("draft-home", true);
    let dir = first.with_extension("drafts");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("Wolf Winter.md");
    fs::write(
        &file,
        "Wolf Winter\nby Sam\n\n# Chapter One\n\nThe snow came early.\n\n***\n\nNobody left the valley.\n\n# Chapter Two\n\nThaw.\n\nTHE END\n",
    )
    .unwrap();
    let mut d = Desk::open(first.clone(), 120, 35);
    d.app.run_action(palette::Action::BringInDraft);
    d.draw();
    assert!(d.shows("BRING IN A DRAFT"));
    // Dropped into the terminal: quoted, spaces and all.
    d.typed(&format!("'{}'", file.display()));
    assert!(
        d.shows("bring it in as a new book"),
        "{}",
        d.rows().join("\n")
    );
    d.key(KeyCode::Enter);

    let root = d.app.project.root.clone();
    assert_ne!(root, fs::canonicalize(&first).unwrap(), "{}", d.status());
    assert_eq!(d.app.project.meta.title, "Wolf Winter");
    assert!(
        d.status().contains("brought in Wolf Winter"),
        "{}",
        d.status()
    );
    assert!(
        d.status().contains("2 chapters, 3 scenes"),
        "{}",
        d.status()
    );
    let scenes: Vec<String> = d
        .app
        .project
        .nodes
        .iter()
        .filter(|n| n.kind == grimoire_core::project::Kind::Scene && n.in_manuscript)
        .map(|n| n.body.trim().to_string())
        .collect();
    assert_eq!(
        scenes,
        ["The snow came early.", "Nobody left the valley.", "Thaw."]
    );
    let notes = fs::read_dir(root.join("notes"))
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect::<String>();
    assert!(notes.contains("by Sam"), "the title page is kept, in Notes");
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn start_a_new_book_also_takes_a_draft() {
    let first = book("draft-new", true);
    let file = first.with_extension("draft.txt");
    fs::write(&file, "CHAPTER ONE\n\nIt began with a letter.\n").unwrap();
    let mut d = Desk::open(first.clone(), 120, 35);
    d.app.run_action(palette::Action::NewBook);
    d.typed(&file.display().to_string());
    assert!(d.shows("bring it in as a new book"));
    d.key(KeyCode::Enter);
    assert!(
        d.status().contains("1 chapter, 1 scene, 5 words"),
        "{}",
        d.status()
    );
    let root = d.app.project.root.clone();
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_file(&file);
}

#[test]
fn progress_shows_the_weeks_and_the_pace_and_a_click_on_the_count_opens_it() {
    let root = book("progress", true);
    let today = chrono::Local::now().date_naive();
    for (back, written) in [(20i64, 900i64), (13, 1200), (6, 600)] {
        grimoire_core::days::record(
            &root,
            grimoire_core::days::Day {
                date: today - chrono::Duration::days(back),
                total: 5000,
                written,
            },
        )
        .unwrap();
    }
    let mut d = Desk::open(root.clone(), 120, 40);
    d.key(KeyCode::Esc);
    d.choose("Writing tools");
    d.choose("Progress…");
    assert!(d.shows("PROGRESS"), "{}", d.rows().join("\n"));
    assert!(d.shows("Each week"));
    assert!(d.shows("this week"));
    assert!(
        d.shows("words a week"),
        "a pace from the weeks before this one"
    );
    assert!(!d.shows("streak"));
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None);

    // Writing is written down, for next time.
    d.key(KeyCode::Tab);
    d.typed("Seven new words for the record today.");
    d.app.note_day(true);
    let record = grimoire_core::days::read(&root);
    let last = record.last().unwrap();
    assert_eq!(last.date, today);
    assert!(last.written >= 7, "{last:?}");

    // The count at the bottom left is a way in.
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    let bottom = d.app.screen.1 - 1;
    d.app.on_click(3, bottom);
    assert_eq!(d.app.overlay, Overlay::Progress);
}

#[test]
fn tab_picks_a_short_story_and_it_starts_with_three_chapters() {
    let first = book("shape-home", true);
    let name = format!("grimoire-ui-shape-{}", std::process::id());
    let story = first.parent().unwrap().join(&name);
    let _ = fs::remove_dir_all(&story);
    let mut d = Desk::open(first.clone(), 120, 35);
    d.app.run_action(palette::Action::NewBook);
    d.draw();
    assert!(d.shows("● Novel"));
    d.key(KeyCode::Tab);
    assert!(d.shows("● Short story"), "{}", d.rows().join("\n"));
    assert!(d.shows("a beginning, a middle and an end"));
    d.typed(&name);
    d.key(KeyCode::Enter);
    assert!(d.status().contains("(short story)"), "{}", d.status());
    let p = &d.app.project;
    let top: Vec<String> = fs::read_dir(story.join("manuscript"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(top.len(), 3, "three chapters, no parts: {top:?}");
    assert!(top.iter().all(|n| n.contains("Chapter")), "{top:?}");
    let scenes = p
        .nodes
        .iter()
        .filter(|n| n.kind == grimoire_core::project::Kind::Scene && n.in_manuscript)
        .count();
    assert_eq!(scenes, 9);
    assert_eq!(p.meta.target_words, 5_000);
    let _ = fs::remove_dir_all(&story);
}

/// Tick the reading until `done` holds, or give up.
fn until(d: &mut Desk, done: impl Fn(&Desk) -> bool) -> bool {
    for _ in 0..200 {
        d.app.tick_reading();
        d.draw();
        if done(d) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

#[test]
fn read_aloud_highlights_each_sentence_and_the_keys_skip_pause_and_stop() {
    let root = book("read-aloud", true);
    fs::write(
        first_scene(&root),
        "---\ntitle: \"Scene One\"\nstatus: draft\n---\n\nThe rain had stopped. Oren was late.\n\nShe waited.\n",
    )
    .unwrap();
    let mut d = Desk::open(root, 120, 35);
    d.key(KeyCode::Tab);
    d.key(KeyCode::Esc);
    d.choose("Writing tools");
    d.choose("Read aloud");
    assert_eq!(d.app.overlay, Overlay::Reading);
    assert!(d.shows("reading aloud"), "{}", d.status());
    assert!(d.shows("sentence 1 of 3"), "{}", d.status());
    assert_eq!(
        d.app.editor.selected_text().as_deref(),
        Some("The rain had stopped.")
    );
    d.key(KeyCode::Right);
    assert!(until(&mut d, |d| d.app.editor.selected_text().as_deref()
        == Some("Oren was late.")));
    d.key(KeyCode::Char(' '));
    assert!(d.status().contains("paused"), "{}", d.status());
    d.key(KeyCode::Char(' '));
    d.key(KeyCode::Right);
    assert!(until(&mut d, |d| d.app.editor.selected_text().as_deref()
        == Some("She waited.")));
    // Past the last sentence: the end of the scene.
    d.key(KeyCode::Right);
    assert!(until(&mut d, |d| d.app.reading.is_none()));
    assert!(d.status().contains("end of the scene"), "{}", d.status());
    assert!(!d.app.editor.has_selection());
    assert_eq!(d.app.overlay, Overlay::None);

    // Esc stops it anywhere.
    d.app.run_action(palette::Action::ReadAloud);
    d.draw();
    d.key(KeyCode::Esc);
    assert!(d.app.reading.is_none());
    assert!(d.status().contains("stopped reading"));
}

#[test]
fn the_reading_voice_is_picked_in_settings() {
    let mut d = Desk::open(book("voices", true), 120, 35);
    d.key(KeyCode::Esc);
    d.choose("Settings");
    d.choose("Reading voice…");
    assert!(d.shows("READING VOICE"));
    assert!(d.shows("Automatic: the most natural one here"));
    d.key(KeyCode::Down);
    d.key(KeyCode::Enter);
    assert!(
        d.status().contains("Read aloud will use Silent"),
        "{}",
        d.status()
    );
}

#[test]
fn goals_take_a_month_of_drafting_preset_and_save_to_novel_toml() {
    let root = book("goals", true);
    let mut d = Desk::open(root.clone(), 120, 35);
    d.key(KeyCode::Esc);
    d.choose("This book");
    d.choose("Goals…");
    assert!(d.shows("GOALS"));
    assert!(d.shows("A month of drafting"));
    assert!(!d.shows("NaNo"));
    d.keys(&[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    assert!(d.shows("50000"), "{}", d.rows().join("\n"));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::None);
    assert!(d.status().contains("goals saved"), "{}", d.status());
    assert_eq!(d.app.project.meta.target_words, 50_000);
    assert_eq!(d.app.project.meta.daily_target, 1_667);
    let toml = fs::read_to_string(root.join("novel.toml")).unwrap();
    assert!(toml.contains("target_words = 50000"), "{toml}");
    assert!(toml.contains("daily_target = 1667"), "{toml}");
    assert!(
        toml.contains("part_label"),
        "everything else is kept: {toml}"
    );
    // And typed by hand.
    d.app.run_action(palette::Action::Goals);
    d.draw();
    d.keys(&[
        KeyCode::Backspace,
        KeyCode::Backspace,
        KeyCode::Backspace,
        KeyCode::Backspace,
    ]);
    d.typed("2000");
    d.key(KeyCode::Enter);
    assert_eq!(d.app.project.meta.target_words, 52_000);
    // Esc leaves them as they were, empty box or not.
    d.app.run_action(palette::Action::Goals);
    for _ in 0..6 {
        d.key(KeyCode::Backspace);
    }
    d.key(KeyCode::Esc);
    assert_eq!(d.app.overlay, Overlay::None);
    assert_eq!(d.app.project.meta.target_words, 52_000);
}

#[test]
fn the_corkboard_is_named_in_the_outline_hints_and_the_menu() {
    let mut d = Desk::open(book("cork-hint", true), 150, 40);
    assert_eq!(d.app.focus, Focus::Tree);
    assert!(d.status().contains("b corkboard"), "{}", d.status());
    d.key(KeyCode::Esc);
    d.choose("This book");
    d.choose("Corkboard…");
    assert!(matches!(d.app.overlay, Overlay::Cork { .. }));
}

#[test]
fn cards_are_added_renamed_moved_and_deleted_from_the_corkboard() {
    let root = book("cork-edit", true);
    let mut d = Desk::open(root, 150, 44);
    d.key(KeyCode::Char('b'));
    assert!(matches!(d.app.overlay, Overlay::Cork { .. }));
    assert!(d.shows("n new"), "{}", d.rows().join("\n"));
    let scenes = |d: &Desk| {
        d.app
            .project
            .nodes
            .iter()
            .filter(|n| n.kind == grimoire_core::project::Kind::Scene && n.in_manuscript)
            .count()
    };
    let before = scenes(&d);

    // A new card, named, and the board comes back with it highlighted.
    d.key(KeyCode::Char('n'));
    assert!(matches!(d.app.overlay, Overlay::Create { .. }));
    d.typed("The Lighthouse");
    d.key(KeyCode::Enter);
    assert!(
        matches!(d.app.overlay, Overlay::Cork { .. }),
        "back to the board"
    );
    assert_eq!(scenes(&d), before + 1);
    let card = |d: &Desk| {
        let Overlay::Cork { sel, scope, .. } = &d.app.overlay else {
            panic!("not the board")
        };
        let groups = grimoire_core::cork::board(&d.app.project, d.app.cork_scope(scope));
        let cards: Vec<_> = groups.iter().flat_map(|g| g.cards.clone()).collect();
        d.app.project.nodes[cards[*sel].idx].title.clone()
    };
    assert_eq!(card(&d), "The Lighthouse");

    // Moved one place earlier, still highlighted.
    d.key(KeyCode::Char('<'));
    assert!(matches!(d.app.overlay, Overlay::Cork { .. }));
    assert_eq!(card(&d), "The Lighthouse");

    // Renamed.
    d.key(KeyCode::Char('r'));
    assert!(matches!(d.app.overlay, Overlay::Rename { .. }));
    d.typed("The Pier");
    d.key(KeyCode::Enter);
    assert_eq!(card(&d), "The Pier");

    // To the Trash, asking first; a no leaves it.
    d.key(KeyCode::Char('d'));
    assert!(matches!(d.app.overlay, Overlay::Confirm { .. }));
    d.key(KeyCode::Char('n'));
    assert!(matches!(d.app.overlay, Overlay::Cork { .. }));
    assert_eq!(scenes(&d), before + 1);
    d.key(KeyCode::Char('d'));
    d.key(KeyCode::Char('y'));
    assert!(matches!(d.app.overlay, Overlay::Cork { .. }));
    assert_eq!(scenes(&d), before);
}

#[test]
fn find_names_history_and_moving_are_in_the_menu_and_hints() {
    let mut d = Desk::open(book("pass", true), 150, 40);
    let labels = |rows: Vec<(String, palette::Action)>| -> Vec<String> {
        rows.into_iter().map(|(l, _)| l).collect()
    };
    let bookrows = labels(d.app.submenu(Sub::Book));
    for want in [
        "Corkboard…",
        "Move up",
        "Move down",
        "Scene history…",
        "Search the whole book…",
        "Writing sessions…",
    ] {
        assert!(
            bookrows.iter().any(|l| l.starts_with(want)),
            "no {want} in {bookrows:?}"
        );
    }
    d.key(KeyCode::Tab);
    let tools = labels(d.app.submenu(Sub::Writing));
    for want in ["Find & replace…", "Check names…", "Read aloud"] {
        assert!(
            tools.iter().any(|l| l.starts_with(want)),
            "no {want} in {tools:?}"
        );
    }
    assert!(d.status().contains("^F find"), "{}", d.status());
    assert!(d.status().contains("^T notes"), "{}", d.status());
}

#[test]
fn ctrl_v_pastes_what_was_copied_and_ctrl_a_selects_the_scene() {
    let mut d = Desk::open(book("paste", true), 120, 35);
    d.key(KeyCode::Tab);
    d.typed("The lamp was lit.");
    d.ctrl('a');
    assert!(
        d.app
            .editor
            .selected_text()
            .is_some_and(|t| t.contains("The lamp was lit."))
    );
    d.app.editor.select((0, 4), (0, 8));
    d.ctrl('c');
    d.app.editor.clear_selection();
    d.key(KeyCode::End);
    d.ctrl('v');
    assert!(
        d.app.editor.lines[0].ends_with("lit.lamp"),
        "{:?}",
        d.app.editor.lines[0]
    );
    // Into a box being typed in, too.
    d.key(KeyCode::Esc);
    d.app.run_action(palette::Action::NewBook);
    d.ctrl('v');
    assert!(matches!(&d.app.overlay, Overlay::NewBook { buf, .. } if buf == "lamp"));
}

#[test]
fn ctrl_b_and_ctrl_i_mark_the_word_at_the_cursor() {
    let mut d = Desk::open(book("emph", true), 120, 35);
    d.key(KeyCode::Tab);
    d.typed("very late");
    d.app.editor.place(0, 2);
    d.ctrl('i');
    assert!(
        d.app.editor.lines[0].starts_with("*very* late"),
        "{:?}",
        d.app.editor.lines[0]
    );
    d.ctrl('i');
    assert!(
        d.app.editor.lines[0].starts_with("very late"),
        "again takes it off"
    );
    d.app.editor.place(0, 7);
    d.ctrl('b');
    assert!(
        d.app.editor.lines[0].starts_with("very **late**"),
        "{:?}",
        d.app.editor.lines[0]
    );
    // Both are in Writing tools, for terminals where Ctrl-I is Tab.
    let tools: Vec<String> = d
        .app
        .submenu(Sub::Writing)
        .into_iter()
        .map(|(l, _)| l)
        .collect();
    assert!(
        tools.iter().any(|l| l.starts_with("Italic"))
            && tools.iter().any(|l| l.starts_with("Bold"))
    );
}

#[test]
fn u_in_the_trash_puts_a_deleted_scene_back_where_it_was() {
    let root = book("restore", true);
    let scene = first_scene(&root);
    let mut d = Desk::open(root.clone(), 150, 40);
    // Delete Scene One (the tree starts on it after Novel Format).
    for _ in 0..30 {
        if d.app
            .visible
            .get(d.app.sel)
            .is_some_and(|&i| d.app.project.nodes[i].path == scene)
        {
            break;
        }
        d.key(KeyCode::Down);
    }
    d.key(KeyCode::Char('d'));
    d.key(KeyCode::Char('y'));
    assert!(!scene.exists());
    // Highlight it in the Trash.
    let trashed = d
        .app
        .project
        .nodes
        .iter()
        .position(|n| n.path.to_string_lossy().ends_with("-01-Scene-One.md"))
        .expect("in the Trash");
    d.app.reveal(trashed);
    d.draw();
    assert!(d.app.selection_in_trash(), "found it in the Trash");
    assert!(d.status().contains("u restore"), "{}", d.status());
    d.key(KeyCode::Char('u'));
    assert!(scene.exists(), "back where it was: {}", d.status());
    assert!(d.status().contains("restored"), "{}", d.status());
}

#[test]
fn the_keyboard_selects_and_moves_a_word_at_a_time() {
    let mut d = Desk::open(book("words", true), 120, 35);
    d.key(KeyCode::Tab);
    d.typed("She didn't look back.");
    let mods = |d: &mut Desk, code: KeyCode, m: KeyModifiers| d.press(KeyEvent::new(code, m));
    // Ctrl-← twice from the end: the start of "look".
    mods(&mut d, KeyCode::Left, KeyModifiers::CONTROL);
    mods(&mut d, KeyCode::Left, KeyModifiers::CONTROL);
    assert_eq!(d.app.editor.cx, 11);
    // Shift-Ctrl-→ selects the word.
    mods(
        &mut d,
        KeyCode::Right,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(d.app.editor.selected_text().as_deref(), Some("look"));
    // Shift-← trims it by a letter.
    mods(&mut d, KeyCode::Left, KeyModifiers::SHIFT);
    assert_eq!(d.app.editor.selected_text().as_deref(), Some("loo"));
    // Ctrl-Backspace at the end takes the last word.
    mods(&mut d, KeyCode::End, KeyModifiers::CONTROL);
    mods(&mut d, KeyCode::Backspace, KeyModifiers::CONTROL);
    assert!(
        d.app.editor.lines[0].starts_with("She didn't look "),
        "{:?}",
        d.app.editor.lines[0]
    );
    // A Mac's Option-← is Alt-b: a word back, never a typed "b".
    let before = d.app.editor.lines[0].clone();
    mods(&mut d, KeyCode::Char('b'), KeyModifiers::ALT);
    mods(&mut d, KeyCode::Char('x'), KeyModifiers::ALT);
    assert_eq!(d.app.editor.lines[0], before, "nothing typed");
    // AltGr (Ctrl+Alt on Windows) types what it makes.
    mods(
        &mut d,
        KeyCode::Char('@'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    assert!(
        d.app.editor.lines[0].contains('@'),
        "{:?}",
        d.app.editor.lines[0]
    );
}

#[test]
fn a_double_click_picks_a_word_and_boxes_on_top_keep_their_clicks() {
    let mut d = Desk::open(book("clicks", true), 120, 35);
    d.key(KeyCode::Tab);
    d.typed("The lighthouse keeper slept.");
    d.draw();
    let (x, y) = d
        .rows()
        .iter()
        .enumerate()
        .find_map(|(y, row)| {
            row.find("lighthouse")
                .map(|b| (row[..b].chars().count() as u16 + 2, y as u16))
        })
        .expect("on screen");
    d.app.on_click(x, y);
    d.app.on_click(x, y);
    d.draw();
    assert_eq!(d.app.editor.selected_text().as_deref(), Some("lighthouse"));
    d.app.on_click(x, y);
    assert!(
        d.app
            .editor
            .selected_text()
            .is_some_and(|t| t.starts_with("The lighthouse"))
    );

    // With the menu up, a click on the outline behind it does nothing, and
    // the wheel moves the menu.
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    let sel = d.app.sel;
    let t = d.app.rect_tree;
    d.app.on_click(t.x + 2, t.y + 3);
    assert!(d.menu_open());
    assert_eq!(d.app.sel, sel);
    d.app.on_scroll(t.x + 2, t.y + 3, true);
    assert_eq!(d.app.overlay, Overlay::Menu { sel: 1 });
}

#[test]
fn details_set_status_pov_target_and_leave_a_scene_out_of_exports() {
    let root = book("details", true);
    let scene = first_scene(&root);
    let mut d = Desk::open(root, 120, 40);
    d.key(KeyCode::Tab);
    d.key(KeyCode::Esc);
    d.choose("Writing tools");
    d.choose("Scene details…");
    assert!(d.shows("DETAILS · SCENE ONE"), "{}", d.rows().join("\n"));
    d.key(KeyCode::Right); // status: outline -> draft
    d.key(KeyCode::Down);
    d.typed("Wren");
    d.keys(&[KeyCode::Down, KeyCode::Down]);
    for _ in 0..4 {
        d.key(KeyCode::Backspace);
    }
    d.typed("2500");
    d.key(KeyCode::Down);
    d.key(KeyCode::Char(' ')); // in exports: no
    d.key(KeyCode::Enter);
    assert!(d.status().contains("details saved"), "{}", d.status());
    d.app.commit_saves();
    let text = fs::read_to_string(&scene).unwrap();
    for want in [
        "status: draft",
        "pov: Wren",
        "target: 2500",
        "compile: false",
    ] {
        assert!(text.contains(want), "{want} in {text}");
    }
    // Esc leaves it as it was.
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    d.app.run_action(palette::Action::Details);
    d.key(KeyCode::Right);
    d.key(KeyCode::Esc);
    d.app.commit_saves();
    assert!(
        fs::read_to_string(&scene)
            .unwrap()
            .contains("status: draft")
    );
}

#[test]
fn spelling_can_be_british_canadian_or_australian() {
    let mut d = Desk::open(book("lang", true), 120, 40);
    d.app.load_speller_now();
    assert!(!d.app.speller.as_ref().unwrap().is_correct("colour"));
    d.key(KeyCode::Esc);
    d.choose("Settings");
    d.choose("Spelling: American English");
    // A setting stays open to show its new state.
    assert!(matches!(
        d.app.overlay,
        Overlay::Sub {
            sub: Sub::Settings,
            ..
        }
    ));
    assert!(
        d.shows("Spelling: British English"),
        "{}",
        d.rows().join("\n")
    );
    assert!(d.app.speller.as_ref().unwrap().is_correct("colour"));
    assert!(d.app.speller.as_ref().unwrap().is_correct("realise"));
}

#[test]
fn nothing_in_ctrl_k_is_missing_from_the_menu() {
    use palette::Action as A;
    let mut d = Desk::open(book("ctrlk-menu", true), 120, 40);
    d.key(KeyCode::Tab); // writing, so the writing tools are all offered
    let mut menu: Vec<A> = d.app.menu().into_iter().map(|(_, a)| a).collect();
    for sub in [
        Sub::Book,
        Sub::Writing,
        Sub::Productivity,
        Sub::Settings,
        Sub::Help,
    ] {
        menu.extend(d.app.submenu(sub).into_iter().map(|(_, a)| a));
    }
    let missing: Vec<String> = palette::entries(&d.app)
        .into_iter()
        .filter(|e| {
            !matches!(
                e.action,
                // Keys every writer knows, shown in the hints and Every key.
                A::Undo | A::Redo | A::Save | A::Paste | A::SelectAll | A::Menu
                    | A::SpellingSuggestions
                    // The music pane's own keys.
                    | A::PlayPause | A::NextTrack | A::PrevTrack
                    // Things by name: scenes, notes, themes, books, topics.
                    | A::Open(_) | A::Theme(_) | A::OpenBookAt(_) | A::Beside(_) | A::HelpTopic(_)
                    // Offered where you'd look for them: Open another book.
                    | A::BringInDraft
            )
        })
        .filter(|e| !menu.contains(&e.action))
        .map(|e| e.label)
        .collect();
    assert!(missing.is_empty(), "only in Ctrl-K: {missing:#?}");
}

// ---- WritingDeck appliance foundation --------------------------------------

#[test]
fn writingdeck_dashboard_is_navigable_at_required_terminal_sizes() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = Desk::open(book(&format!("deck-home-{w}"), true), w, h);
        let before = d.app.editor.text();
        d.key(KeyCode::F(12));
        assert!(d.shows("WRITINGDECK OS"));
        assert!(d.shows("Resume writing"));
        assert!(d.shows("local save is not backup"));
        d.key(KeyCode::End);
        assert!(d.shows("▸ Configure widgets"));
        d.key(KeyCode::Enter);
        assert!(d.shows("Dashboard widgets"));
        d.key(KeyCode::Esc);
        d.key(KeyCode::Esc);
        assert_eq!(d.app.editor.text(), before);
        assert_eq!(d.app.focus, Focus::Editor);
    }
}

#[test]
fn writingdeck_outline_toggles_without_focus_mode_and_persists() {
    let root = book("deck-outline", true);
    let mut d = Desk::open(root.clone(), 100, 30);
    d.key(KeyCode::F(10));
    assert!(!d.app.deck.state.current().outline);
    assert!(!d.app.focus_mode);
    assert_eq!(d.app.rect_tree.width, 0);
    assert_eq!(d.app.focus, Focus::Editor);
    d.key(KeyCode::Tab);
    assert_eq!(d.app.focus, Focus::Editor);
    let store = grimoire_core::workspaces::Store::load(&root).unwrap();
    assert!(!store.state.current().outline);
    d.key(KeyCode::F(10));
    assert!(d.app.rect_tree.width > 0);
}

#[test]
fn writingdeck_strict_focus_hides_reference_panes_without_closing_them() {
    let mut d = with_wren("deck-focus-codex");
    assert!(d.app.codex.is_some());
    d.ctrl('d');
    assert!(d.app.focus_mode);
    assert_eq!(d.app.rect_codex.width, 0);
    assert_eq!(d.app.rect_beside.width, 0);
    d.key(KeyCode::Tab);
    assert_eq!(d.app.focus, Focus::Editor);
    d.key(KeyCode::Esc);
    assert!(d.menu_open(), "an invisible reference must not eat Escape");
    d.key(KeyCode::Esc);
    d.ctrl('d');
    assert!(
        d.app.codex.is_some(),
        "same note comes back without duplicating it"
    );
    assert!(d.app.rect_codex.width > 0);
}

#[test]
fn writingdeck_widgets_keep_visibility_and_order_after_reload() {
    let mut d = Desk::open(book("deck-widgets", true), 80, 24);
    d.key(KeyCode::F(12));
    let first = d.app.deck.state.dashboard[0].widget;
    d.key(KeyCode::Char('J'));
    assert_eq!(d.app.deck.state.dashboard[1].widget, first);
    d.key(KeyCode::Char('h'));
    let s = grimoire_core::workspaces::Store::load(&d.root).unwrap();
    assert_eq!(s.state.dashboard[1].widget, first);
    assert!(!s.state.dashboard[1].visible);
    d.key(KeyCode::Char('w'));
    d.key(KeyCode::Down);
    d.key(KeyCode::Enter);
    assert!(
        grimoire_core::workspaces::Store::load(&d.root)
            .unwrap()
            .state
            .dashboard[1]
            .visible
    );
}

#[test]
fn writingdeck_workspaces_restore_document_cursor_and_width() {
    use crate::app::writingdeck::Page;
    let mut d = Desk::open(book("deck-workspace", true), 120, 40);
    d.key(KeyCode::Tab);
    d.typed("Original words. ");
    let old_pos = (d.app.editor.cy, d.app.editor.cx);
    d.app.overlay = Overlay::Deck(Page::Workspaces { sel: 1 });
    d.key(KeyCode::Char('+'));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.deck.state.selected, 1);
    assert_eq!(d.app.rect_tree.width + 2, 32);
    d.app.editor.place(0, 0);
    d.app.overlay = Overlay::Deck(Page::Workspaces { sel: 0 });
    d.key(KeyCode::Enter);
    assert_eq!((d.app.editor.cy, d.app.editor.cx), old_pos);
    assert_eq!(d.app.rect_tree.width + 2, 30);
    assert!(d.app.editor.text().contains("Original words."));
}

#[test]
fn writingdeck_custom_shortcuts_are_additive_and_logical() {
    use crate::app::writingdeck::Page;
    for layout in ["qwerty", "colemak", "colemak-dh"] {
        let mut d = Desk::open(book(&format!("deck-shortcuts-{layout}"), true), 100, 30);
        d.app.overlay = Overlay::Deck(Page::Shortcuts {
            sel: 0,
            capture: true,
        });
        d.press(KeyEvent::new(
            KeyCode::Char('g'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));
        d.key(KeyCode::Esc); // system
        d.key(KeyCode::Esc); // home
        d.key(KeyCode::Esc); // prose
        d.press(KeyEvent::new(
            KeyCode::Char('g'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));
        assert!(matches!(d.app.overlay, Overlay::Deck(Page::Home { .. })));
        d.key(KeyCode::Esc);
        d.ctrl('s');
        assert!(
            d.app.msg.contains("saved"),
            "legacy Ctrl-S still works under {layout}"
        );
        d.typed("é € λ ");
        assert!(
            d.app.editor.text().contains("é € λ"),
            "OS-translated Unicode stays unchanged"
        );
    }
}

#[test]
fn writingdeck_recovery_prompt_precedes_startup_dashboard() {
    let root = book("deck-startup-recovery", true);
    recovery::keep(&root, &first_scene(&root), "Recovered WritingDeck words.").unwrap();
    let mut d = Desk::open(root, 80, 24);
    d.app.deck.state.launch_dashboard = true;
    d.app.init_writingdeck(true);
    assert!(matches!(d.app.overlay, Overlay::Recover { .. }));
}

#[test]
fn writingdeck_keyboard_preview_field_is_not_manuscript_text() {
    use crate::app::writingdeck::Page;
    let mut d = Desk::open(book("deck-keyboard-field", true), 80, 24);
    let text = d.app.editor.text();
    d.app.overlay = Overlay::Deck(Page::KeyboardTest {
        text: String::new(),
    });
    d.typed("qwerty arstneio ? €");
    let Overlay::Deck(Page::KeyboardTest { text: field }) = &d.app.overlay else {
        panic!("question marks must stay in the preview field");
    };
    assert_eq!(field, "qwerty arstneio ? €");
    assert_eq!(d.app.editor.text(), text);
    d.key(KeyCode::Esc);
    assert_eq!(d.app.editor.text(), text);
}

#[test]
fn writingdeck_tutorial_uses_actual_editor_and_persists_verified_progress() {
    let root =
        std::env::temp_dir().join(format!("grimoire-deck-tutorial-ui-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    grimoire_core::tutorial::create_demo(&root).unwrap();
    let mut d = Desk::open(root, 80, 24);
    d.app.deck.progress.start(4).unwrap();
    d.app.focus = Focus::Editor;
    d.typed("My own new sentence. ");
    assert!(d.app.deck.progress.completed.contains(&4));
    let saved = grimoire_core::tutorial::Store::load(&d.root).unwrap();
    assert!(saved.progress.completed.contains(&4));
    d.app.deck.progress.start(4).unwrap();
    assert_eq!(d.app.deck.progress.active, Some(4));
    assert!(
        d.app.deck.progress.start(31).is_err(),
        "Continuity remains unavailable until its actual interaction exists"
    );
    d.app.deck.progress.start(25).unwrap();
    assert_eq!(d.app.deck.progress.active, Some(25));
    assert!(!d.app.deck.progress.completed.contains(&25));
}

#[test]
fn writingdeck_every_lesson_links_to_real_searchable_offline_help() {
    for l in grimoire_core::tutorial::lessons() {
        assert!(
            crate::help::TOPICS.iter().any(|t| t.id == l.help_topic),
            "lesson {} has no help",
            l.id
        );
    }
}

#[test]
fn writingdeck_async_system_status_is_explicitly_simulated_and_does_not_block_writing() {
    use crate::app::writingdeck::Page;
    let mut d = Desk::open(book("deck-system-worker", true), 100, 30);
    d.app.deck.simulate();
    d.app.open_system_settings();
    d.key(KeyCode::Esc);
    d.key(KeyCode::Esc);
    d.app.focus = Focus::Editor;
    d.typed("Writing while status loads. ");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while d.app.deck.busy && std::time::Instant::now() < deadline {
        d.app.tick_deck();
        std::thread::yield_now();
    }
    assert!(d.app.deck.snapshot.as_ref().unwrap().simulated);
    assert!(d.app.editor.text().contains("Writing while status loads."));
    d.app.overlay = Overlay::Deck(Page::System { sel: 9 });
    d.key(KeyCode::Enter);
    assert!(matches!(
        d.app.overlay,
        Overlay::Deck(Page::Power { yes: false, .. })
    ));
    assert!(
        !d.app.deck.busy,
        "merely opening confirmation never powers off"
    );
    d.key(KeyCode::Enter); // default cancel
    assert!(!d.app.deck.busy);
}

#[test]
fn writingdeck_bad_layout_state_is_preserved_while_manuscript_still_saves() {
    let root = book("deck-layout-corrupt", true);
    let path = grimoire_core::workspaces::Store::path(&root);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "broken layout").unwrap();
    let mut d = Desk::open(root, 80, 24);
    d.key(KeyCode::Tab);
    d.typed("Words must still save. ");
    d.key(KeyCode::F(10));
    d.ctrl('s');
    assert_eq!(fs::read_to_string(path).unwrap(), "broken layout");
    assert!(
        fs::read_to_string(first_scene(&d.root))
            .unwrap()
            .contains("Words must still save.")
    );
}

// ---- Independent editable documents (WritingDeck M3b) -------------------

fn secondary_book(tag: &str, width: u16, height: u16, many_lines: bool) -> (Desk, PathBuf) {
    let root = book(tag, true);
    let primary = first_scene(&root);
    let body = if many_lines {
        (0..100)
            .map(|i| format!("Primary paragraph {i}.\n"))
            .collect::<String>()
    } else {
        "Primary alpha remains here.\n".into()
    };
    fs::write(
        &primary,
        format!("---\ntitle: Scene One\nkeep: 'raw: value' # unchanged\n---\n\n{body}"),
    )
    .unwrap();
    let note = root.join("research/side-research.md");
    let body = if many_lines {
        (0..100)
            .map(|i| format!("Secondary paragraph {i}.\n"))
            .collect::<String>()
    } else {
        "Secondary beta lives here.\n".into()
    };
    fs::write(&note, format!("---\ntitle: Side Research\ncustom:\n  list: [one, two] # retained\nunknown: 'a: b'\n---\n\n{body}")).unwrap();
    (Desk::open(root, width, height), note)
}

fn choose_secondary(d: &mut Desk, note: &Path) {
    d.app
        .run_action(palette::Action::EditSecondary(note.to_path_buf()));
    d.draw();
    assert_eq!(d.app.focus, Focus::Secondary, "{}", d.rows().join("\n"));
}

#[test]
fn writingdeck_secondary_works_at_all_four_sizes_and_saves_raw_frontmatter() {
    use grimoire_core::documents::Pane;
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut d, note) =
            secondary_book(&format!("secondary-sizes-{width}"), width, height, false);
        let primary = first_scene(&d.root);
        let old_primary = fs::read_to_string(&primary).unwrap();
        let (old_front, _) = project::split_frontmatter(&fs::read_to_string(&note).unwrap());
        // Foundation v1 alias files omit Secondary: F11 still has its
        // additive default fallback without migrating the shortcut file.
        d.app
            .deck
            .shortcuts
            .bindings
            .retain(|b| b.action != writingdeck_sys::shortcuts::Action::Secondary);
        d.key(KeyCode::F(11));
        assert!(matches!(d.app.overlay, Overlay::Palette { .. }));
        d.typed("Side Research");
        d.key(KeyCode::Enter);
        assert_eq!(d.app.focus, Focus::Secondary);
        assert!(d.shows("Primary alpha"));
        assert!(d.shows("Secondary beta"));
        assert!(d.app.view_rect(Pane::Primary).height >= 4);
        assert!(d.app.view_rect(Pane::Secondary).height >= 4);
        if width <= 100 {
            assert!(d.app.view_rect(Pane::Secondary).y > d.app.view_rect(Pane::Primary).y);
        } else {
            assert!(d.app.view_rect(Pane::Secondary).x > d.app.view_rect(Pane::Primary).x);
        }
        d.press(KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL));
        d.typed("Café λ — new research.");
        d.ctrl('s');
        assert_eq!(fs::read_to_string(&primary).unwrap(), old_primary);
        let saved = fs::read_to_string(&note).unwrap();
        assert!(saved.contains("Café λ — new research."));
        assert_eq!(project::split_frontmatter(&saved).0, old_front);
        assert_eq!(d.app.project.dirty_count(), 0);
    }
}

#[test]
fn writingdeck_secondary_and_primary_have_independent_undo_selection_and_clipboard() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-independent", 120, 40, false);
    choose_secondary(&mut d, &note);
    let primary = d.app.view_editor(Pane::Primary).unwrap().text();
    let secondary = d.app.editor.text();
    d.press(KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL));
    d.app.clip = "SECONDARY Ω".into();
    d.ctrl('v');
    d.press(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
    assert_eq!(d.app.editor.selected_text().as_deref(), Some("Ω"));
    d.key(KeyCode::BackTab);
    assert_eq!(d.app.focus, Focus::Editor);
    assert_eq!(d.app.editor.text(), primary);
    assert_eq!(
        d.app
            .view_editor(Pane::Secondary)
            .unwrap()
            .selected_text()
            .as_deref(),
        Some("Ω")
    );
    d.press(KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL));
    d.app.clip = "PRIMARY INSERT".into();
    d.ctrl('v');
    let changed_primary = d.app.editor.text();
    d.ctrl('z');
    assert_eq!(d.app.editor.text(), primary);
    d.ctrl('y');
    assert_eq!(d.app.editor.text(), changed_primary);
    d.key(KeyCode::Tab);
    assert_eq!(d.app.focus, Focus::Secondary);
    assert_eq!(d.app.editor.selected_text().as_deref(), Some("Ω"));
    d.ctrl('c');
    assert_eq!(d.app.clip, "Ω");
    d.ctrl('z');
    assert_eq!(d.app.editor.text(), secondary);
    assert_eq!(
        d.app.view_editor(Pane::Primary).unwrap().text(),
        changed_primary
    );
    d.ctrl('y');
    assert!(d.app.editor.text().ends_with("SECONDARY Ω"));
    d.ctrl('a');
    d.ctrl('x');
    assert_eq!(d.app.editor.text(), "");
    d.ctrl('z');
    assert!(d.app.editor.text().ends_with("SECONDARY Ω"));
    d.ctrl('s');
    assert!(
        fs::read_to_string(first_scene(&d.root))
            .unwrap()
            .contains("PRIMARY INSERT")
    );
    assert!(fs::read_to_string(note).unwrap().contains("SECONDARY Ω"));
}

#[test]
fn writingdeck_secondary_hiding_and_strict_focus_never_discard_or_focus_hidden_text() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-focus", 80, 24, false);
    choose_secondary(&mut d, &note);
    d.app.editor.select((0, 0), (0, 9));
    let selection = d.app.editor.selected_text();
    d.key(KeyCode::F(11));
    assert_eq!(
        d.app.view_rect(Pane::Secondary),
        ratatui::layout::Rect::default()
    );
    assert_eq!(
        d.app.view_editor(Pane::Secondary).unwrap().selected_text(),
        selection
    );
    for _ in 0..10 {
        d.key(KeyCode::Tab);
        assert_ne!(d.app.focus, Focus::Secondary);
    }
    d.key(KeyCode::F(11));
    assert_eq!(d.app.focus, Focus::Secondary);
    assert_eq!(d.app.editor.selected_text(), selection);
    d.ctrl('d');
    assert!(d.app.focus_mode);
    assert_eq!(d.app.views.active, Pane::Primary);
    assert_eq!(
        d.app.view_rect(Pane::Secondary),
        ratatui::layout::Rect::default()
    );
    for _ in 0..5 {
        d.key(KeyCode::Tab);
        assert_eq!(d.app.focus, Focus::Editor);
    }
    d.key(KeyCode::F(11));
    assert!(d.app.focus_mode);
    d.ctrl('d');
    assert!(d.app.views.secondary_visible);
    d.key(KeyCode::Tab);
    assert_eq!(d.app.focus, Focus::Secondary);
    assert_eq!(d.app.editor.selected_text(), selection);
    d.key(KeyCode::Esc); // selection first, not a hidden legacy note
    assert_eq!(d.app.overlay, Overlay::None);
    assert!(!d.app.editor.has_selection());
    d.key(KeyCode::Esc);
    assert!(d.menu_open());
}

#[test]
fn writingdeck_secondary_mouse_wheel_and_click_use_each_views_geometry() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-mouse", 100, 30, true);
    choose_secondary(&mut d, &note);
    let primary_rect = d.app.view_rect(Pane::Primary);
    let secondary_rect = d.app.view_rect(Pane::Secondary);
    let secondary_scroll = d.app.editor.scroll;
    d.app
        .on_scroll(primary_rect.x + 4, primary_rect.y + 3, true);
    d.draw();
    assert_eq!(d.app.focus, Focus::Secondary);
    assert!(d.app.view_editor(Pane::Primary).unwrap().scroll > 0);
    assert_eq!(d.app.editor.scroll, secondary_scroll);
    d.app.on_click(primary_rect.x + 5, primary_rect.y + 3);
    d.draw();
    assert_eq!(d.app.focus, Focus::Editor);
    let primary_cursor = (d.app.editor.cy, d.app.editor.cx);
    d.app.on_click(secondary_rect.x + 5, secondary_rect.y + 3);
    d.draw();
    assert_eq!(d.app.focus, Focus::Secondary);
    assert_eq!(
        (
            d.app.view_editor(Pane::Primary).unwrap().cy,
            d.app.view_editor(Pane::Primary).unwrap().cx
        ),
        primary_cursor
    );
    d.app.on_drag(secondary_rect.x + 12, secondary_rect.y + 3);
    d.draw();
    assert!(d.app.editor.has_selection());
    assert!(!d.app.view_editor(Pane::Primary).unwrap().has_selection());
}

#[test]
fn writingdeck_secondary_autosave_also_saves_a_hidden_inactive_dirty_document() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-autosave", 120, 40, false);
    choose_secondary(&mut d, &note);
    d.app.clip = "note auto ".into();
    d.ctrl('v');
    d.key(KeyCode::F(11));
    d.app.clip = "primary auto ".into();
    d.ctrl('v');
    assert_eq!(d.app.project.dirty_count(), 2);
    d.app.last_edit = Some(std::time::Instant::now() - std::time::Duration::from_secs(3));
    d.app.autosave_tick();
    assert_eq!(d.app.project.dirty_count(), 0);
    assert!(fs::read_to_string(&note).unwrap().contains("note auto"));
    assert!(
        fs::read_to_string(first_scene(&d.root))
            .unwrap()
            .contains("primary auto")
    );
    assert_eq!(d.app.views.active, Pane::Primary);
    assert!(!grimoire_core::history::versions(&d.root, &note).is_empty());
}

#[test]
fn writingdeck_secondary_find_replace_and_spelling_do_not_touch_primary() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-find-spell", 120, 40, false);
    choose_secondary(&mut d, &note);
    let primary = d.app.view_editor(Pane::Primary).unwrap().text();
    d.ctrl('f');
    d.typed("beta");
    assert_eq!(d.app.editor.selected_text().as_deref(), Some("beta"));
    d.key(KeyCode::Tab);
    d.typed("gamma");
    d.key(KeyCode::Enter);
    d.key(KeyCode::Esc);
    assert_eq!(d.app.focus, Focus::Secondary);
    assert!(d.app.editor.text().contains("gamma"));
    assert!(!d.app.editor.text().contains("beta"));
    d.ctrl('z');
    assert!(d.app.editor.text().contains("beta"));
    d.app.load_speller_now();
    d.app.editor.place(0, 0);
    d.app.clip = "definately ".into();
    d.ctrl('v');
    d.app.editor.place(0, 4);
    d.key(KeyCode::F(8));
    assert!(matches!(d.app.overlay, Overlay::Spelling { .. }));
    d.key(KeyCode::Esc);
    assert_eq!(d.app.focus, Focus::Secondary);
    assert_eq!(d.app.view_editor(Pane::Primary).unwrap().text(), primary);
}

#[test]
fn writingdeck_secondary_duplicate_selection_focuses_owner_instead_of_cloning_buffer() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-owner", 120, 40, false);
    choose_secondary(&mut d, &note);
    let p = first_scene(&d.root);
    let before = d.app.editor.text();
    d.app.editor.select((0, 2), (0, 8));
    d.app.run_action(palette::Action::EditSecondary(p.clone()));
    d.draw();
    assert_eq!(d.app.focus, Focus::Editor);
    assert_eq!(d.app.view_editor(Pane::Secondary).unwrap().text(), before);
    d.app.run_action(palette::Action::Open(note.clone()));
    d.draw();
    assert_eq!(d.app.focus, Focus::Secondary);
    assert_eq!(d.app.editor.selected_text().as_deref(), Some("condar"));
    assert_eq!(
        d.app.project.nodes[d.app.view_open(Pane::Primary).unwrap()].path,
        p
    );
    d.app.ensure_document_claims().unwrap();
}

#[test]
fn writingdeck_secondary_conflict_keeps_external_version_and_local_copy_without_reflush() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-conflict", 120, 40, false);
    choose_secondary(&mut d, &note);
    d.app.clip = "Local secondary draft. ".into();
    d.ctrl('v');
    d.key(KeyCode::BackTab);
    d.app.clip = "Local primary draft. ".into();
    d.ctrl('v');
    fs::write(
        &note,
        "---\ntitle: Side Research\nexternal: retained\n---\n\nExternal secondary revision.\n",
    )
    .unwrap();
    d.ctrl('s');
    assert!(
        fs::read_to_string(&note)
            .unwrap()
            .contains("External secondary revision.")
    );
    let copies = grimoire_core::sync::copies_of(&note);
    assert_eq!(copies.len(), 1);
    assert!(
        fs::read_to_string(&copies[0].0)
            .unwrap()
            .contains("Local secondary draft.")
    );
    assert!(
        d.app
            .view_editor(Pane::Secondary)
            .unwrap()
            .text()
            .contains("External secondary revision.")
    );
    assert!(
        fs::read_to_string(first_scene(&d.root))
            .unwrap()
            .contains("Local primary draft.")
    );
    d.app.check_documents_now();
    d.draw();
    d.ctrl('s');
    assert_eq!(grimoire_core::sync::copies_of(&note).len(), 1);
    assert!(
        d.app
            .view_editor(Pane::Secondary)
            .unwrap()
            .text()
            .contains("External secondary revision.")
    );
}

#[test]
fn writingdeck_secondary_clean_external_adoption_and_tree_reload_preserve_other_selection() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-external", 120, 40, false);
    choose_secondary(&mut d, &note);
    d.key(KeyCode::BackTab);
    d.app.editor.select((0, 1), (0, 5));
    let selection = d.app.editor.selected_text();
    fs::write(
        &note,
        "---\ntitle: Side Research\n---\n\nA clean external revision.\n",
    )
    .unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    assert!(
        d.app
            .view_editor(Pane::Secondary)
            .unwrap()
            .text()
            .contains("clean external")
    );
    assert_eq!(d.app.editor.selected_text(), selection);
    let extra = d.root.join("notes/extra.md");
    fs::write(extra, "An additional note.\n").unwrap();
    d.app.check_documents_now();
    d.draw();
    assert_eq!(d.app.editor.selected_text(), selection);
    assert!(
        d.app
            .view_editor(Pane::Secondary)
            .unwrap()
            .text()
            .contains("clean external")
    );
}

#[test]
fn writingdeck_secondary_failed_save_blocks_replacement_and_retains_both_recovery_copies() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-save-fail", 120, 40, false);
    choose_secondary(&mut d, &note);
    d.app.clip = "Unsaved secondary. ".into();
    d.ctrl('v');
    d.key(KeyCode::BackTab);
    d.app.clip = "Unsaved primary. ".into();
    d.ctrl('v');
    let secondary_index = d.app.view_open(Pane::Secondary);
    let backup = note.with_extension("fixture-backup");
    fs::rename(&note, &backup).unwrap();
    fs::create_dir(&note).unwrap();
    d.ctrl('s');
    assert!(matches!(d.app.save_state, app::SaveState::Failed(_)));
    let primary = first_scene(&d.root);
    let kept = recovery::dir(&d.root);
    assert!(
        fs::read_to_string(kept.join(primary.strip_prefix(&d.root).unwrap()))
            .unwrap()
            .contains("Unsaved primary.")
    );
    assert!(
        fs::read_to_string(kept.join(note.strip_prefix(&d.root).unwrap()))
            .unwrap()
            .contains("Unsaved secondary.")
    );
    let third = d
        .app
        .project
        .nodes
        .iter()
        .position(|n| {
            n.kind == grimoire_core::project::Kind::Scene && n.path != primary && n.path != note
        })
        .unwrap();
    d.app.open_secondary_node(third);
    assert_eq!(d.app.view_open(Pane::Secondary), secondary_index);
    d.ctrl('q');
    assert!(!d.left);
    assert!(d.armed);
    assert!(
        d.app
            .view_editor(Pane::Secondary)
            .unwrap()
            .text()
            .contains("Unsaved secondary.")
    );
    fs::remove_dir(&note).unwrap();
    fs::rename(&backup, &note).unwrap();
    assert!(d.app.commit_saves());
    assert!(
        fs::read_to_string(&primary)
            .unwrap()
            .contains("Unsaved primary.")
    );
    assert!(
        fs::read_to_string(&note)
            .unwrap()
            .contains("Unsaved secondary.")
    );
}

#[test]
fn writingdeck_secondary_workspace_restart_restores_both_selections_free_scroll_size_and_focus() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-restart", 120, 40, true);
    choose_secondary(&mut d, &note);
    d.app.with_editor_view(Pane::Primary, |a| {
        a.editor.select((12, 8), (9, 4));
    });
    d.app.editor.select((10, 3), (8, 7));
    // Scroll deliberately away from both cursors, then save those views.
    d.app.scroll_editor_view(Pane::Primary, true, 45);
    d.app.scroll_editor_view(Pane::Secondary, true, 38);
    d.app.deck.state.current_mut().secondary_percent = 55;
    d.draw();
    let primary = d.app.view_position(Pane::Primary).unwrap();
    let secondary = d.app.view_position(Pane::Secondary).unwrap();
    assert!(primary.scroll > primary.line);
    assert!(secondary.scroll > secondary.line);
    assert!(d.app.try_quit());
    let mut reopened = Desk::open(d.root.clone(), 120, 40);
    reopened.app.init_writingdeck(true);
    reopened.draw();
    assert_eq!(reopened.app.deck.state.version, 3);
    assert_eq!(reopened.app.view_position(Pane::Primary).unwrap(), primary);
    assert_eq!(
        reopened.app.view_position(Pane::Secondary).unwrap(),
        secondary
    );
    assert_eq!(reopened.app.focus, Focus::Secondary);
    assert_eq!(reopened.app.deck.state.current().secondary_percent, 55);
    assert!(reopened.app.views.secondary_visible);
}

#[test]
fn writingdeck_secondary_restore_never_overrides_a_newer_primary_resume_record() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-newer-resume", 120, 40, true);
    choose_secondary(&mut d, &note);
    d.app
        .with_editor_view(Pane::Primary, |a| a.editor.place(9, 4));
    d.app.editor.select((6, 1), (7, 3));
    let secondary = d.app.view_position(Pane::Secondary).unwrap();
    d.app.save_deck();
    let record = grimoire_core::resume::Resume {
        scene: grimoire_core::resume::relative(&d.root, &first_scene(&d.root)),
        line: 40,
        column: 2,
        machine: "another fixture writer".into(),
        when: chrono::Local::now(),
    };
    grimoire_core::resume::write(&d.root, &record, "Scene One", "").unwrap();
    let mut reopened = Desk::open(d.root.clone(), 120, 40);
    reopened.app.init_writingdeck(true);
    reopened.draw();
    assert_eq!(reopened.app.view_editor(Pane::Primary).unwrap().cy, 40);
    assert_eq!(reopened.app.view_editor(Pane::Primary).unwrap().cx, 2);
    assert_eq!(
        reopened
            .app
            .view_editor(Pane::Primary)
            .unwrap()
            .selection_anchor(),
        None
    );
    assert_eq!(
        reopened.app.view_position(Pane::Secondary).unwrap(),
        secondary
    );
    assert_eq!(reopened.app.focus, Focus::Editor);
}

#[test]
fn writingdeck_secondary_workspace_can_swap_roles_without_duplicate_editors_or_lost_undo() {
    use app::writingdeck::Page;
    use grimoire_core::{documents::Pane, workspaces::Position};
    let (mut d, note) = secondary_book("secondary-role-swap", 120, 40, false);
    choose_secondary(&mut d, &note);
    let primary = first_scene(&d.root);
    d.app.clip = "NOTE HISTORY ".into();
    d.ctrl('v');
    let note_text = d.app.editor.text();
    d.key(KeyCode::BackTab);
    d.app.editor.select((0, 1), (0, 5));
    let primary_position = d.app.view_position(Pane::Primary).unwrap();
    let plan = &mut d.app.deck.state.workspaces[1];
    plan.primary = Position {
        document: Some(note.strip_prefix(&d.root).unwrap().to_path_buf()),
        ..Position::default()
    };
    plan.secondary = Some(Position {
        document: Some(primary.strip_prefix(&d.root).unwrap().to_path_buf()),
        ..Position::default()
    });
    plan.secondary_visible = true;
    plan.secondary_percent = 60;
    plan.active_editor = Pane::Primary;
    d.app.overlay = Overlay::Deck(Page::Workspaces { sel: 1 });
    d.key(KeyCode::Enter);
    assert_eq!(
        d.app.project.nodes[d.app.view_open(Pane::Primary).unwrap()].path,
        note
    );
    assert_eq!(
        d.app.project.nodes[d.app.view_open(Pane::Secondary).unwrap()].path,
        primary
    );
    assert_eq!(d.app.view_editor(Pane::Primary).unwrap().text(), note_text);
    d.ctrl('z');
    assert!(!d.app.editor.text().contains("NOTE HISTORY"));
    d.ctrl('y');
    assert_eq!(d.app.editor.text(), note_text);
    d.app.ensure_document_claims().unwrap();
    d.app.overlay = Overlay::Deck(Page::Workspaces { sel: 0 });
    d.key(KeyCode::Enter);
    assert_eq!(
        d.app.view_position(Pane::Primary).unwrap(),
        primary_position
    );
    assert_eq!(
        d.app.project.nodes[d.app.view_open(Pane::Secondary).unwrap()].path,
        note
    );
    assert_eq!(
        d.app.view_editor(Pane::Secondary).unwrap().text(),
        note_text
    );
}

#[test]
fn writingdeck_secondary_book_wide_replace_and_undo_update_inactive_buffers_too() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-replace-book", 120, 40, false);
    choose_secondary(&mut d, &note);
    d.key(KeyCode::BackTab);
    d.ctrl('f');
    d.typed("here");
    d.ctrl('f');
    assert!(matches!(d.app.overlay, Overlay::FindBook { .. }));
    d.key(KeyCode::Tab);
    d.typed("there");
    d.ctrl('r');
    d.key(KeyCode::Char('y'));
    for pane in [Pane::Primary, Pane::Secondary] {
        assert!(d.app.view_editor(pane).unwrap().text().contains("there."));
    }
    d.ctrl('z');
    for pane in [Pane::Primary, Pane::Secondary] {
        assert!(!d.app.view_editor(pane).unwrap().text().contains("there."));
    }
    d.ctrl('y');
    for pane in [Pane::Primary, Pane::Secondary] {
        assert!(d.app.view_editor(pane).unwrap().text().contains("there."));
    }
    d.ctrl('s');
    assert!(fs::read_to_string(note).unwrap().contains("there."));
}

#[cfg(unix)]
#[test]
fn writingdeck_secondary_external_alias_change_pauses_save_and_disk_adoption_without_data_loss() {
    use grimoire_core::documents::Pane;
    use std::os::unix::fs::symlink;
    let (mut d, note) = secondary_book("secondary-alias-change", 120, 40, false);
    choose_secondary(&mut d, &note);
    let original_note = fs::read_to_string(&note).unwrap();
    let primary = first_scene(&d.root);
    let original_primary = fs::read_to_string(&primary).unwrap();
    d.app.clip = "Unique secondary changes. ".into();
    d.ctrl('v');
    d.key(KeyCode::BackTab);
    d.app.clip = "Unique primary changes. ".into();
    d.ctrl('v');
    let secondary_text = d.app.view_editor(Pane::Secondary).unwrap().text();
    fs::remove_file(&note).unwrap();
    symlink(&primary, &note).unwrap();
    d.app.check_documents_now();
    d.draw();
    d.ctrl('s');
    assert!(matches!(d.app.save_state, app::SaveState::Failed(_)));
    assert_eq!(
        d.app.view_editor(Pane::Secondary).unwrap().text(),
        secondary_text
    );
    assert!(d.app.editor.text().contains("Unique primary changes."));
    assert_eq!(fs::read_to_string(&primary).unwrap(), original_primary);
    assert!(grimoire_core::sync::copies_of(&note).is_empty());
    let primary_index = d.app.view_open(Pane::Primary);
    let third = d
        .app
        .project
        .nodes
        .iter()
        .find(|n| {
            n.kind == grimoire_core::project::Kind::Scene && n.path != primary && n.path != note
        })
        .unwrap()
        .path
        .clone();
    d.app.run_action(palette::Action::Open(third));
    d.draw();
    assert_eq!(d.app.view_open(Pane::Primary), primary_index);
    assert!(
        d.app
            .view_editor(Pane::Primary)
            .unwrap()
            .text()
            .contains("Unique primary changes.")
    );
    fs::remove_file(&note).unwrap();
    fs::write(&note, original_note).unwrap();
    assert!(d.app.commit_saves());
    assert!(
        fs::read_to_string(&note)
            .unwrap()
            .contains("Unique secondary changes.")
    );
    assert!(
        fs::read_to_string(&primary)
            .unwrap()
            .contains("Unique primary changes.")
    );
}

#[test]
fn writingdeck_secondary_rename_and_trash_leave_primary_safe_and_keep_undo_recoverable() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-rename-trash", 120, 40, false);
    choose_secondary(&mut d, &note);
    d.app.clip = "Keep this note edit. ".into();
    d.ctrl('v');
    let primary = first_scene(&d.root);
    let primary_text = fs::read_to_string(&primary).unwrap();
    d.app.run_action(palette::Action::Rename);
    d.draw();
    d.typed("Renamed research");
    d.key(KeyCode::Enter);
    let renamed = d.app.project.nodes[d.app.view_open(Pane::Secondary).unwrap()]
        .path
        .clone();
    assert_ne!(renamed, note);
    assert!(renamed.is_file());
    assert!(
        d.app
            .view_editor(Pane::Secondary)
            .unwrap()
            .text()
            .contains("Keep this note edit.")
    );
    d.app.ensure_document_claims().unwrap();
    d.app.run_action(palette::Action::Delete);
    d.draw();
    d.key(KeyCode::Char('y'));
    assert_eq!(d.app.view_open(Pane::Secondary), None);
    assert_eq!(fs::read_to_string(&primary).unwrap(), primary_text);
    assert!(!renamed.exists());
    assert!(in_trash(&d.root, "Keep this note edit."));
    d.ctrl('z');
    assert!(renamed.is_file());
    d.app.run_action(palette::Action::EditSecondary(renamed));
    d.draw();
    d.ctrl('z');
    assert!(!d.app.editor.text().contains("Keep this note edit."));
    assert_eq!(
        d.app.project.nodes[d.app.view_open(Pane::Primary).unwrap()].path,
        primary
    );
}

#[test]
fn writingdeck_secondary_resize_never_types_into_a_hidden_view_and_preserves_its_buffer() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-resize", 120, 40, false);
    choose_secondary(&mut d, &note);
    d.app.clip = "Survives resize. ".into();
    d.ctrl('v');
    d.app.editor.select((0, 1), (0, 6));
    let text = d.app.editor.text();
    let selection = d.app.editor.selected_text();
    d.term = Terminal::new(TestBackend::new(60, 8)).unwrap();
    d.draw();
    assert!(!d.app.views.secondary_room);
    assert_ne!(d.app.focus, Focus::Secondary);
    assert_eq!(
        d.app.view_rect(Pane::Secondary),
        ratatui::layout::Rect::default()
    );
    for _ in 0..6 {
        d.key(KeyCode::Tab);
        assert_ne!(d.app.focus, Focus::Secondary);
    }
    assert_eq!(d.app.view_editor(Pane::Secondary).unwrap().text(), text);
    d.term = Terminal::new(TestBackend::new(160, 50)).unwrap();
    d.draw();
    d.key(KeyCode::F(11));
    d.key(KeyCode::F(11));
    assert_eq!(d.app.focus, Focus::Secondary);
    assert_eq!(d.app.editor.selected_text(), selection);
    d.ctrl('z');
    assert!(!d.app.editor.text().contains("Survives resize."));
}

#[test]
fn writingdeck_secondary_crash_rescue_and_recovery_restore_both_original_documents() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-crash-recovery", 120, 40, false);
    choose_secondary(&mut d, &note);
    d.app.clip = "Recovered secondary text. ".into();
    d.ctrl('v');
    d.key(KeyCode::BackTab);
    d.app.clip = "Recovered primary text. ".into();
    d.ctrl('v');
    d.app.save_deck();
    d.app.rescue();
    // Simulate the original process going away: a new App reads only files.
    let mut restarted = Desk::open(d.root.clone(), 120, 40);
    restarted.app.init_writingdeck(true);
    restarted.draw();
    let Overlay::Recover { items, .. } = &restarted.app.overlay else {
        panic!("both drafts must be offered");
    };
    assert_eq!(items.len(), 2);
    restarted.key(KeyCode::Enter);
    assert_eq!(restarted.app.overlay, Overlay::None);
    assert!(
        restarted
            .app
            .view_editor(Pane::Primary)
            .unwrap()
            .text()
            .contains("Recovered primary text.")
    );
    assert!(
        restarted
            .app
            .view_editor(Pane::Secondary)
            .unwrap()
            .text()
            .contains("Recovered secondary text.")
    );
    assert!(
        fs::read_to_string(&note)
            .unwrap()
            .contains("Recovered secondary text.")
    );
    assert!(
        fs::read_to_string(first_scene(&d.root))
            .unwrap()
            .contains("Recovered primary text.")
    );
    assert!(recovery::pending(&Project::load(&d.root).unwrap()).is_empty());
}

#[test]
fn writingdeck_secondary_tutorial_requires_a_real_demo_edit_not_opening_or_navigation() {
    let root = std::env::temp_dir().join(format!(
        "grimoire-secondary-tutorial-{}",
        std::process::id()
    ));
    grimoire_core::tutorial::create_demo(&root).unwrap();
    let mut d = Desk::open(root, 80, 24);
    d.app.deck.progress.start(13).unwrap();
    d.key(KeyCode::F(11));
    d.typed("Other Door");
    d.key(KeyCode::Enter);
    assert_eq!(d.app.focus, Focus::Secondary);
    assert!(!d.app.deck.progress.completed.contains(&13));
    d.key(KeyCode::Right);
    assert!(!d.app.deck.progress.completed.contains(&13));
    d.typed("My secondary sentence. ");
    assert!(d.app.deck.progress.completed.contains(&13));
    assert!(
        grimoire_core::tutorial::Store::load(&d.root)
            .unwrap()
            .progress
            .completed
            .contains(&13)
    );
    d.app.deck.progress.start(13).unwrap();
    d.app.clip = "A pasted exercise sentence.".into();
    d.ctrl('v');
    assert!(d.app.deck.progress.completed.contains(&13));
}

#[test]
fn writingdeck_secondary_tutorial_will_not_launch_a_writing_exercise_in_the_real_novel() {
    use app::writingdeck::Page;
    let (mut d, note) = secondary_book("secondary-no-real-tutorial", 120, 40, false);
    let before = fs::read_to_string(first_scene(&d.root)).unwrap();
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 13 });
    d.key(KeyCode::Enter);
    assert!(d.app.deck.note.contains("safe demo"));
    assert_eq!(d.app.deck.progress.active, None);
    assert!(!d.app.views.secondary_visible);
    assert_eq!(fs::read_to_string(first_scene(&d.root)).unwrap(), before);
    assert!(note.is_file());
}

#[test]
fn writingdeck_secondary_external_rename_follows_both_views_and_preserves_clean_selections() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-external-rename", 120, 40, false);
    choose_secondary(&mut d, &note);
    d.app.editor.select((0, 2), (0, 7));
    let selection = d.app.editor.selected_text();
    let renamed = note.with_file_name("externally-renamed.md");
    fs::rename(&note, &renamed).unwrap();
    d.key(KeyCode::BackTab);
    d.app.check_documents_now();
    d.draw();
    assert_eq!(
        d.app.project.nodes[d.app.view_open(Pane::Secondary).unwrap()].path,
        renamed
    );
    assert_eq!(
        d.app.view_editor(Pane::Secondary).unwrap().selected_text(),
        selection
    );
    let primary = first_scene(&d.root);
    let moved = primary.with_file_name("externally-moved.md");
    fs::rename(primary, &moved).unwrap();
    d.app.check_documents_now();
    d.draw();
    assert_eq!(
        d.app.project.nodes[d.app.view_open(Pane::Primary).unwrap()].path,
        moved
    );
    assert!(d.app.editor.text().contains("Primary alpha"));
    d.app.ensure_document_claims().unwrap();
    d.ctrl('s');
}

#[test]
fn writingdeck_secondary_dirty_external_rename_parks_local_words_instead_of_overwriting_moved_file()
{
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-dirty-rename", 120, 40, false);
    choose_secondary(&mut d, &note);
    let external = fs::read_to_string(&note).unwrap();
    d.app.clip = "Dirty words before rename. ".into();
    d.ctrl('v');
    d.key(KeyCode::BackTab);
    let moved = note.with_file_name("moved-while-typing.md");
    fs::rename(&note, &moved).unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    assert_eq!(fs::read_to_string(&moved).unwrap(), external);
    assert!(in_trash(&d.root, "Dirty words before rename."));
    assert_eq!(
        d.app.project.nodes[d.app.view_open(Pane::Secondary).unwrap()].path,
        moved
    );
    assert!(
        !d.app
            .view_editor(Pane::Secondary)
            .unwrap()
            .text()
            .contains("Dirty words before rename.")
    );
    d.ctrl('s');
    assert_eq!(fs::read_to_string(moved).unwrap(), external);
}

#[test]
fn writingdeck_secondary_ambiguous_external_move_never_attaches_to_an_arbitrary_duplicate() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-ambiguous-rename", 120, 40, false);
    choose_secondary(&mut d, &note);
    let moved = note.with_file_name("first-copy.md");
    let duplicate = note.with_file_name("second-copy.md");
    fs::rename(&note, &moved).unwrap();
    fs::copy(&moved, &duplicate).unwrap();
    let before = fs::read(&moved).unwrap();
    d.app.check_documents_now();
    d.draw();
    assert_eq!(d.app.view_open(Pane::Secondary), None);
    assert_ne!(d.app.focus, Focus::Secondary);
    d.typed("This cannot silently change a guessed note. ");
    d.ctrl('s');
    assert_eq!(fs::read(moved).unwrap(), before);
    assert_eq!(fs::read(duplicate).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn writingdeck_secondary_existing_symlink_and_hardlink_aliases_focus_the_owner() {
    use grimoire_core::documents::Pane;
    use std::os::unix::fs::symlink;
    let (mut d, note) = secondary_book("secondary-known-aliases", 120, 40, false);
    choose_secondary(&mut d, &note);
    let primary = first_scene(&d.root);
    let symlink_path = d.root.join("notes/primary-symbolic-alias.md");
    let hardlink_path = d.root.join("notes/primary-hard-alias.md");
    symlink(&primary, &symlink_path).unwrap();
    fs::hard_link(&primary, &hardlink_path).unwrap();
    d.app.check_documents_now();
    d.draw();
    for alias in [symlink_path, hardlink_path] {
        d.app.activate_editor(Pane::Secondary);
        d.app
            .run_action(palette::Action::EditSecondary(alias.clone()));
        d.draw();
        assert_eq!(d.app.focus, Focus::Editor);
        d.app.editor.select((0, 2), (0, 7));
        let position = d.app.view_position(Pane::Primary).unwrap();
        d.app.run_action(palette::Action::Open(alias));
        d.draw();
        assert_eq!(d.app.view_position(Pane::Primary).unwrap(), position);
        assert_eq!(
            d.app.project.nodes[d.app.view_open(Pane::Secondary).unwrap()].path,
            note
        );
        assert_eq!(
            d.app.project.nodes[d.app.view_open(Pane::Primary).unwrap()].path,
            primary
        );
    }
}

#[test]
fn writingdeck_secondary_and_primary_refuse_non_utf8_edits_and_never_rewrite_lossy_bytes() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-read-only", 120, 40, false);
    choose_secondary(&mut d, &note);
    let primary = first_scene(&d.root);
    let invalid = b"Not UTF8 \xff raw bytes.\n";
    fs::write(&primary, invalid).unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    d.key(KeyCode::BackTab);
    d.typed("MUST NOT SAVE");
    assert!(d.app.msg.contains("UTF-8"));
    d.ctrl('s');
    assert_eq!(fs::read(&primary).unwrap(), invalid);
    d.key(KeyCode::Tab);
    assert_eq!(d.app.focus, Focus::Secondary);
    fs::write(&note, invalid).unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    d.typed("ALSO MUST NOT SAVE");
    assert!(d.app.msg.contains("UTF-8"));
    d.ctrl('s');
    assert_eq!(fs::read(&note).unwrap(), invalid);
    let secondary_index = d.app.view_open(Pane::Secondary).unwrap();
    d.app.run_action(palette::Action::Open(note.clone()));
    d.draw();
    assert_eq!(d.app.view_open(Pane::Secondary), Some(secondary_index));
    assert_eq!(d.app.focus, Focus::Secondary);
    assert_ne!(d.app.view_open(Pane::Primary), Some(secondary_index));
    #[cfg(unix)]
    {
        let alias = d.root.join("notes/read-only-note-alias.md");
        fs::hard_link(&note, &alias).unwrap();
        d.app.check_documents_now();
        d.draw();
        let secondary_index = d.app.view_open(Pane::Secondary).unwrap();
        d.app.run_action(palette::Action::Open(alias.clone()));
        d.draw();
        assert_eq!(d.app.focus, Focus::Secondary);
        assert_ne!(d.app.view_open(Pane::Primary), Some(secondary_index));
        fs::write(&note, "Now valid UTF-8 text.\n").unwrap();
        d.app.check_documents_now();
        d.app.check_documents_now();
        d.draw();
        d.app.run_action(palette::Action::Open(alias));
        d.draw();
        assert_eq!(d.app.focus, Focus::Secondary);
        assert_eq!(d.app.view_open(Pane::Secondary), Some(secondary_index));
        d.typed("Accepted UTF-8 edit. ");
        d.ctrl('s');
        assert!(
            fs::read_to_string(&note)
                .unwrap()
                .contains("Accepted UTF-8 edit.")
        );
    }
    assert_eq!(fs::read(primary).unwrap(), invalid);
}

#[test]
fn writingdeck_secondary_settling_an_open_conflict_copy_closes_it_before_trashing() {
    use grimoire_core::documents::Pane;
    let (root, original, copy) = conflicted("secondary-settle-open-copy");
    let mut d = Desk::open(root, 140, 42);
    d.app.run_action(palette::Action::Open(original.clone()));
    d.draw();
    choose_secondary(&mut d, &copy);
    let copy_text = d.app.editor.text();
    d.app.open_conflicts();
    d.draw();
    d.key(KeyCode::Enter);
    assert!(matches!(d.app.overlay, Overlay::Settle { .. }));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.view_open(Pane::Secondary), None);
    assert_ne!(d.app.focus, Focus::Secondary);
    assert!(!copy.exists());
    assert!(
        fs::read_to_string(&original)
            .unwrap()
            .contains(copy_text.trim())
    );
    d.ctrl('z');
    assert!(copy.is_file());
}

#[test]
fn writingdeck_secondary_history_restore_refuses_non_utf8_and_cannot_replace_the_other_view() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("secondary-history-read-only", 120, 40, false);
    choose_secondary(&mut d, &note);
    let primary = d.app.view_editor(Pane::Primary).unwrap().text();
    grimoire_core::history::snapshot(&d.root, &note, "An old valid UTF-8 draft.\n", None).unwrap();
    let invalid = b"Undecodable \xff note.\n";
    fs::write(&note, invalid).unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    d.app.run_action(palette::Action::History);
    d.draw();
    assert!(matches!(d.app.overlay, Overlay::History { .. }));
    d.key(KeyCode::Enter);
    assert!(d.app.msg.contains("UTF-8"));
    assert_eq!(fs::read(note).unwrap(), invalid);
    assert_eq!(d.app.view_editor(Pane::Primary).unwrap().text(), primary);
}

#[test]
fn writingdeck_secondary_recovery_refuses_non_utf8_without_discarding_the_kept_draft() {
    let (mut d, note) = secondary_book("secondary-recovery-non-utf8", 120, 40, false);
    choose_secondary(&mut d, &note);
    recovery::keep(&d.root, &note, "Unique recovered note words.\n").unwrap();
    let invalid = b"Raw \xff bytes are still here.\n";
    fs::write(&note, invalid).unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    let items = recovery::pending(&d.app.project);
    assert_eq!(items.len(), 1);
    let kept = items[0].file.clone();
    d.app.overlay = Overlay::Recover { items, sel: 0 };
    d.key(KeyCode::Enter);
    assert!(d.app.msg.contains("left in recovery"));
    assert_eq!(fs::read(&note).unwrap(), invalid);
    assert!(
        fs::read_to_string(kept)
            .unwrap()
            .contains("Unique recovered note words.")
    );
}

// ---- Metadata-only sticky references (M4a) -------------------------------
fn pin_document(d: &mut Desk, source: &Path) -> u64 {
    d.app
        .run_action(palette::Action::PinDocument(source.to_path_buf()));
    d.draw();
    assert_eq!(d.app.overlay, Overlay::Pins, "{}", d.app.msg);
    d.app.pins.state().selected.unwrap()
}

#[test]
fn writingdeck_pins_real_source_edit_save_and_undo_at_all_four_sizes() {
    use grimoire_core::documents::Pane;
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut d, note) = secondary_book(&format!("pins-size-{w}"), w, h, true);
        let primary = first_scene(&d.root);
        let primary_before = fs::read(&primary).unwrap();
        let note_before = fs::read(&note).unwrap();
        let id = pin_document(&mut d, &note);
        assert!(d.shows("Secondary paragraph 0."), "{}", d.rows().join("\n"));
        let position = d.app.view_position(Pane::Primary).unwrap();
        d.key(KeyCode::PageDown);
        assert_eq!(d.app.pins.state().pins[0].scroll, 3);
        assert_eq!(d.app.view_position(Pane::Primary).unwrap(), position);
        let metadata = fs::read_to_string(grimoire_core::pins::Store::path(&d.root)).unwrap();
        assert!(!metadata.contains("Secondary paragraph"));
        assert!(!metadata.contains("unknown: 'a: b'"));
        assert_eq!(fs::read(&note).unwrap(), note_before);
        d.key(KeyCode::Enter);
        assert_eq!(d.app.focus, Focus::Secondary, "{}", d.app.msg);
        d.app.clip = "Pin source changes here. ".into();
        d.ctrl('v');
        d.ctrl('s');
        assert!(
            fs::read_to_string(&note)
                .unwrap()
                .contains("Pin source changes here.")
        );
        assert!(
            fs::read_to_string(&note)
                .unwrap()
                .contains("unknown: 'a: b'")
        );
        assert_eq!(fs::read(&primary).unwrap(), primary_before);
        assert!(
            d.app
                .pin_body(id)
                .unwrap()
                .contains("Pin source changes here.")
        );
        d.ctrl('z');
        d.ctrl('s');
        assert_eq!(fs::read(&note).unwrap(), note_before);
    }
}

#[test]
fn writingdeck_pins_duplicate_cards_scroll_independently_and_keep_both_editors_positions() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("pins-independent", 160, 50, true);
    choose_secondary(&mut d, &note);
    d.app.editor.select((8, 1), (8, 7));
    d.draw();
    d.app.activate_editor(Pane::Primary);
    d.app.editor.select((5, 2), (5, 9));
    d.draw();
    let first = pin_document(&mut d, &note);
    let second = pin_document(&mut d, &note);
    d.key(KeyCode::Esc);
    d.draw();
    assert!(d.app.pins.rect.width > 0);
    let primary = d.app.view_position(Pane::Primary).unwrap();
    let secondary = d.app.view_position(Pane::Secondary).unwrap();
    let focus = d.app.focus;
    let rect = d
        .app
        .pins
        .hits
        .iter()
        .find(|(id, _)| *id == first)
        .unwrap()
        .1;
    d.app.on_scroll(rect.x + 1, rect.y + 2, true);
    d.draw();
    assert_eq!(
        d.app
            .pins
            .state()
            .pins
            .iter()
            .find(|p| p.id == first)
            .unwrap()
            .scroll,
        3
    );
    assert_eq!(
        d.app
            .pins
            .state()
            .pins
            .iter()
            .find(|p| p.id == second)
            .unwrap()
            .scroll,
        0
    );
    assert_eq!(d.app.focus, focus);
    assert_eq!(d.app.view_position(Pane::Primary).unwrap(), primary);
    assert_eq!(d.app.view_position(Pane::Secondary).unwrap(), secondary);
    d.app.on_click(rect.x + 1, rect.y + 1);
    d.draw();
    assert_eq!(d.app.focus, Focus::Pins);
    d.key(KeyCode::Enter);
    assert_eq!(d.app.focus, Focus::Secondary);
    assert_eq!(d.app.view_position(Pane::Secondary).unwrap(), secondary);
}

#[test]
fn writingdeck_pins_narrow_tiny_and_strict_focus_hide_without_losing_cards() {
    let (mut d, note) = secondary_book("pins-fallback", 160, 50, true);
    pin_document(&mut d, &note);
    d.key(KeyCode::Esc);
    d.draw();
    let rect = d.app.pins.hits[0].1;
    d.app.on_click(rect.x + 1, rect.y + 1);
    d.draw();
    assert_eq!(d.app.focus, Focus::Pins);
    let state = d.app.pins.state().clone();
    d.term = Terminal::new(TestBackend::new(60, 8)).unwrap();
    d.draw();
    assert_eq!(d.app.pins.rect, ratatui::layout::Rect::default());
    assert_ne!(d.app.focus, Focus::Pins);
    for _ in 0..10 {
        d.key(KeyCode::Tab);
        assert_ne!(d.app.focus, Focus::Pins);
    }
    assert_eq!(d.app.pins.state(), &state);
    d.app.run_action(palette::Action::Pinboard);
    d.draw();
    assert!(d.shows("PINBOARD"));
    d.key(KeyCode::Esc);
    d.term = Terminal::new(TestBackend::new(160, 50)).unwrap();
    d.draw();
    d.ctrl('d');
    assert!(d.app.focus_mode);
    assert_eq!(d.app.pins.rect, ratatui::layout::Rect::default());
    for _ in 0..5 {
        d.key(KeyCode::Tab);
        assert_ne!(d.app.focus, Focus::Pins);
    }
    d.ctrl('d');
    assert!(d.app.pins.rect.width > 0);
    assert_eq!(d.app.pins.state(), &state);
}

#[test]
fn writingdeck_pins_presentation_order_archive_trash_and_restart_are_metadata_only() {
    use grimoire_core::pins::{Color, Life, Store};
    let (mut d, note) = secondary_book("pins-persist", 160, 50, true);
    let original = fs::read(&note).unwrap();
    let a = pin_document(&mut d, &note);
    let b = pin_document(&mut d, &note);
    d.key(KeyCode::PageDown);
    d.key(KeyCode::Char('c'));
    d.key(KeyCode::Char('g'));
    d.typed("Ideas");
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('+'));
    d.key(KeyCode::Char(']'));
    d.key(KeyCode::Char(' '));
    d.key(KeyCode::Char('K'));
    let pin = &d.app.pins.state().pins[0];
    assert_eq!(pin.id, b);
    assert_eq!(pin.scroll, 3);
    assert_eq!(pin.category, "Ideas");
    assert_eq!(pin.color, Color::Amber);
    assert_eq!(pin.height, 10);
    assert!(pin.collapsed);
    assert_eq!(d.app.pins.state().sidebar_width, 32);
    d.key(KeyCode::Char('a'));
    assert_eq!(d.app.pins.state().pins[0].life, Life::Archived);
    d.key(KeyCode::Tab);
    d.key(KeyCode::Char('d'));
    assert_eq!(d.app.pins.state().pins[0].life, Life::Trashed);
    d.key(KeyCode::Tab);
    d.key(KeyCode::Char('u'));
    assert_eq!(d.app.pins.state().pins[0].life, Life::Active);
    assert_eq!(d.app.pins.state().pins[1].id, a);
    assert_eq!(fs::read(&note).unwrap(), original);
    let state = d.app.pins.state().clone();
    assert_eq!(Store::load(&d.root).unwrap().state, state);
    let reopened = Desk::open(d.root.clone(), 160, 50);
    assert_eq!(reopened.app.pins.state(), &state);
}

#[test]
fn writingdeck_pins_unknown_corrupt_and_future_metadata_are_preserved_not_reset() {
    use grimoire_core::pins::{State, Store};
    let (mut d, note) = secondary_book("pins-invalid", 120, 40, false);
    let path = Store::path(&d.root);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let future = State {
        version: 99,
        ..State::default()
    };
    let mut unknown = serde_json::to_value(State::default()).unwrap();
    unknown["future"] = true.into();
    for original in [
        "not JSON".into(),
        serde_json::to_string(&future).unwrap(),
        unknown.to_string(),
    ] {
        fs::write(&path, &original).unwrap();
        d.app.pins = app::pins::Pins::load(&d.root);
        d.app.run_action(palette::Action::Pinboard);
        d.draw();
        assert!(d.app.pins.warning.contains("preserved"));
        d.app.run_action(palette::Action::PinDocument(note.clone()));
        d.draw();
        assert_eq!(d.app.overlay, Overlay::Pins);
        assert!(d.app.pins.state().pins.is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}

#[test]
fn writingdeck_pins_external_metadata_conflict_rolls_back_until_explicit_reload() {
    use grimoire_core::pins::Store;
    let (mut d, note) = secondary_book("pins-conflict", 120, 40, true);
    pin_document(&mut d, &note);
    let old = d.app.pins.state().clone();
    let path = Store::path(&d.root);
    let mut external = old.clone();
    external.pins[0].category = "Other writer".into();
    let text = serde_json::to_string_pretty(&external).unwrap();
    fs::write(&path, &text).unwrap();
    d.key(KeyCode::PageDown);
    assert_eq!(d.app.pins.state(), &old);
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    assert!(d.app.pins.warning.contains("NOT saved"));
    d.key(KeyCode::Char('R'));
    assert_eq!(d.app.pins.state(), &external);
    assert!(d.app.pins.warning.is_empty());
    d.key(KeyCode::PageDown);
    assert_eq!(Store::load(&d.root).unwrap().state.pins[0].scroll, 3);
}

#[test]
fn writingdeck_pins_metadata_write_failure_keeps_previous_cards_and_manuscripts() {
    let (mut d, note) = secondary_book("pins-write-fail", 120, 40, true);
    pin_document(&mut d, &note);
    let original = fs::read(&note).unwrap();
    let old = d.app.pins.state().clone();
    let dir = d.root.join(".grimoire/writingdeck");
    let parked = dir.with_extension("kept");
    fs::rename(&dir, &parked).unwrap();
    fs::write(&dir, "directory unavailable").unwrap();
    d.key(KeyCode::PageDown);
    assert_eq!(d.app.pins.state(), &old);
    assert!(d.app.pins.warning.contains("NOT saved"));
    assert_eq!(fs::read(&note).unwrap(), original);
    fs::remove_file(&dir).unwrap();
    fs::rename(&parked, &dir).unwrap();
    d.key(KeyCode::Char('R'));
    d.key(KeyCode::PageDown);
    assert_eq!(d.app.pins.state().pins[0].scroll, 3);
}

#[test]
fn writingdeck_pins_unique_heading_tracks_insertions_and_missing_or_duplicate_is_explicit() {
    use grimoire_core::pins::{Mode, Resolution};
    let (mut d, note) = secondary_book("pins-heading", 120, 40, true);
    fs::write(
        &note,
        format!(
            "## Plan\n{}",
            (0..60).map(|i| format!("Step {i}.\n")).collect::<String>()
        ),
    )
    .unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    let id = pin_document(&mut d, &note);
    d.key(KeyCode::Char('h'));
    d.key(KeyCode::PageDown);
    assert_eq!(d.app.pins.state().pins[0].mode, Mode::Anchored);
    assert_eq!(d.app.pins.state().pins[0].scroll, 3);
    fs::write(
        &note,
        format!("Inserted above.\n\n{}", fs::read_to_string(&note).unwrap()),
    )
    .unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.key(KeyCode::Home);
    let pin = &d.app.pins.state().pins[0];
    assert_eq!(pin.scroll, 0);
    assert_eq!(
        grimoire_core::pins::resolve(d.app.pin_body(id).unwrap(), pin.anchor.as_ref()),
        Resolution::Found(2)
    );
    fs::write(&note, "## Plan\nOne\n## Plan\nTwo\n").unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    assert!(d.shows("ANCHOR Ambiguous"));
    let old = d.app.pins.state().clone();
    d.key(KeyCode::PageDown);
    assert_eq!(d.app.pins.state(), &old);
    fs::write(&note, "## Removed\nNew body.\n").unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    assert!(d.shows("ANCHOR Missing"));
    d.key(KeyCode::Char('f'));
    assert!(d.shows("New body."));
}

#[test]
fn writingdeck_pins_quote_capture_uses_original_editor_selection_and_does_not_erase_it() {
    use grimoire_core::documents::Pane;
    use grimoire_core::pins::Anchor;
    let (mut d, note) = secondary_book("pins-quote", 120, 40, true);
    choose_secondary(&mut d, &note);
    d.app.editor.select((4, 0), (4, 9));
    d.draw();
    let selected = d.app.editor.selected_text().unwrap();
    let position = d.app.view_position(Pane::Secondary).unwrap();
    pin_document(&mut d, &note);
    d.key(KeyCode::Char('q'));
    assert_eq!(
        d.app.pins.state().pins[0].anchor,
        Some(Anchor::Quote { text: selected })
    );
    assert_eq!(d.app.view_position(Pane::Secondary).unwrap(), position);
    d.key(KeyCode::Enter);
    assert_eq!(d.app.view_position(Pane::Secondary).unwrap(), position);
}

#[test]
fn writingdeck_pins_known_rename_trash_and_undo_follow_without_source_divergence() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("pins-moves", 120, 40, false);
    let id = pin_document(&mut d, &note);
    d.key(KeyCode::Enter);
    d.app.run_action(palette::Action::Rename);
    d.draw();
    d.typed("Pin source renamed");
    d.key(KeyCode::Enter);
    let renamed = d.app.project.nodes[d.app.view_open(Pane::Secondary).unwrap()]
        .path
        .clone();
    assert_ne!(renamed, note);
    assert_eq!(d.root.join(&d.app.pins.state().pins[0].source), renamed);
    assert!(d.app.pin_body(id).is_ok());
    d.app.run_action(palette::Action::Delete);
    d.draw();
    d.key(KeyCode::Char('y'));
    assert!(!renamed.exists());
    assert_eq!(d.app.pins.state().pins.len(), 1);
    assert!(d.app.pin_body(id).is_err());
    d.ctrl('z');
    d.app.check_documents_now();
    d.app.check_documents_now();
    assert!(renamed.exists());
    assert!(d.app.pin_body(id).is_ok());
    assert_eq!(d.root.join(&d.app.pins.state().pins[0].source), renamed);
}

#[test]
fn writingdeck_pins_missing_source_keeps_reference_until_explicit_relink() {
    let (mut d, note) = secondary_book("pins-relink", 120, 40, false);
    let id = pin_document(&mut d, &note);
    let original = d.app.pins.state().pins[0].source.clone();
    fs::remove_file(&note).unwrap();
    d.app.refresh_pin_sources();
    d.draw();
    assert!(d.app.pin_body(id).is_err());
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Pins);
    assert_eq!(d.app.pins.state().pins[0].source, original);
    d.key(KeyCode::Char('b'));
    assert!(matches!(d.app.overlay, Overlay::Palette { .. }));
    d.typed("Scene One");
    d.key(KeyCode::Enter);
    assert_eq!(d.app.pins.state().pins[0].id, id);
    assert_ne!(d.app.pins.state().pins[0].source, original);
    assert!(!note.exists());
    assert!(d.app.pin_body(id).unwrap().contains("Primary alpha"));
}

#[test]
fn writingdeck_pins_existing_primary_and_secondary_owners_retain_undo_and_selection() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("pins-owner", 160, 50, false);
    choose_secondary(&mut d, &note);
    d.typed("Owner draft. ");
    d.app.editor.select((0, 0), (0, 5));
    d.draw();
    let secondary = d.app.view_position(Pane::Secondary).unwrap();
    pin_document(&mut d, &note);
    d.key(KeyCode::Enter);
    assert_eq!(d.app.view_position(Pane::Secondary).unwrap(), secondary);
    assert_eq!(d.app.focus, Focus::Secondary);
    let primary = first_scene(&d.root);
    pin_document(&mut d, &primary);
    d.key(KeyCode::Enter);
    assert_eq!(d.app.focus, Focus::Editor);
    assert_eq!(d.app.view_position(Pane::Secondary).unwrap(), secondary);
    d.app.activate_editor(Pane::Secondary);
    d.ctrl('z');
    assert!(!d.app.editor.text().contains("Owner draft."));
}

#[cfg(unix)]
#[test]
fn writingdeck_pins_symlink_and_hardlink_aliases_read_the_unsaved_owner_not_a_stale_node() {
    use grimoire_core::documents::Pane;
    use std::os::unix::fs::symlink;
    let (mut d, _) = secondary_book("pins-aliases", 160, 50, false);
    let primary = first_scene(&d.root);
    let symbolic = d.root.join("notes/pin-symbolic.md");
    let hard = d.root.join("notes/pin-hard.md");
    symlink(&primary, &symbolic).unwrap();
    fs::hard_link(&primary, &hard).unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.app.activate_editor(Pane::Primary);
    d.typed("Unsaved authoritative words. ");
    let position = d.app.view_position(Pane::Primary).unwrap();
    for alias in [symbolic, hard] {
        let id = pin_document(&mut d, &alias);
        assert!(
            d.app
                .pin_body(id)
                .unwrap()
                .contains("Unsaved authoritative words.")
        );
        d.key(KeyCode::Enter);
        assert_eq!(d.app.focus, Focus::Editor);
        assert_eq!(d.app.view_position(Pane::Primary).unwrap(), position);
        assert!(d.app.view_open(Pane::Secondary).is_none());
    }
}

#[cfg(unix)]
#[test]
fn writingdeck_pins_outside_project_symlink_and_internal_recovery_source_are_refused() {
    use std::os::unix::fs::symlink;
    let (mut d, _) = secondary_book("pins-contained", 120, 40, false);
    let outside = Desk::open(book("pins-outside", true), 80, 24);
    let external = first_scene(&outside.root);
    let alias = d.root.join("research/escape.md");
    symlink(&external, &alias).unwrap();
    let original = fs::read(&external).unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.app.run_action(palette::Action::PinDocument(alias));
    assert!(d.app.pins.state().pins.is_empty());
    assert!(d.app.msg.contains("outside"));
    assert_eq!(fs::read(&external).unwrap(), original);
    let internal = d.root.join(".grimoire/recovery/source.md");
    fs::create_dir_all(internal.parent().unwrap()).unwrap();
    fs::write(&internal, "Internal recovery words.").unwrap();
    d.app.run_action(palette::Action::PinDocument(internal));
    assert!(d.app.pins.state().pins.is_empty());
}

#[test]
fn writingdeck_pins_plain_typing_paste_and_editor_undo_shortcuts_cannot_mutate_hidden_source() {
    let (mut d, note) = secondary_book("pins-read-only", 160, 50, false);
    d.key(KeyCode::Tab);
    d.typed("Primary draft. ");
    let primary = d.app.editor.text();
    let id = pin_document(&mut d, &note);
    d.ctrl('z');
    d.ctrl('y');
    d.ctrl('x');
    d.ctrl('v');
    assert_eq!(d.app.editor.text(), primary);
    d.app.editor.select((0, 0), (0, 7));
    let position = d
        .app
        .view_position(grimoire_core::documents::Pane::Primary)
        .unwrap();
    let rect = d.app.rect_editor;
    d.app.on_drag(rect.x + 2, rect.y + 2);
    assert_eq!(
        d.app
            .view_position(grimoire_core::documents::Pane::Primary)
            .unwrap(),
        position
    );
    d.key(KeyCode::Char('g'));
    d.ctrl('z');
    d.ctrl('x');
    d.app.clip = "Safe category".into();
    d.ctrl('v');
    d.key(KeyCode::Enter);
    assert_eq!(d.app.pins.state().pins[0].category, "Safe category");
    assert_eq!(d.app.editor.text(), primary);
    assert_eq!(
        d.app
            .view_position(grimoire_core::documents::Pane::Primary)
            .unwrap(),
        position
    );
    d.key(KeyCode::Esc);
    let rect = d.app.pins.hits[0].1;
    d.app.on_click(rect.x + 1, rect.y + 1);
    d.draw();
    d.app.paste("Cannot insert this.");
    d.ctrl('z');
    d.ctrl('x');
    assert_eq!(d.app.editor.text(), primary);
    assert!(d.app.pin_body(id).unwrap().contains("Secondary beta"));
}

#[test]
fn writingdeck_pins_scopes_follow_primary_context_not_the_secondary_note() {
    use grimoire_core::pins::Scope;
    let (mut d, note) = secondary_book("pins-scope", 160, 50, false);
    let id = pin_document(&mut d, &note);
    assert_eq!(d.app.pin_ids(false), vec![id]);
    assert!(d.app.pin_ids(true).is_empty());
    d.key(KeyCode::Char('s'));
    assert_eq!(d.app.pins.state().pins[0].scope, Scope::Chapter);
    assert_eq!(d.app.pin_ids(false), vec![id]);
    d.key(KeyCode::Enter);
    assert_eq!(d.app.focus, Focus::Secondary);
    assert_eq!(d.app.pin_ids(false), vec![id]);
    d.app.open_pinboard();
    d.key(KeyCode::Char('s'));
    assert_eq!(d.app.pins.state().pins[0].scope, Scope::Scene);
    assert_eq!(d.app.pin_ids(false), vec![id]);
    d.key(KeyCode::Char('s'));
    assert!(d.app.pin_ids(false).is_empty());
    assert_eq!(d.app.pin_ids(true), vec![id]);
    d.key(KeyCode::Char('s'));
    assert!(d.app.pin_ids(false).is_empty());
    assert!(d.app.pin_ids(true).is_empty());
    d.key(KeyCode::Char('s'));
    assert_eq!(d.app.pin_ids(false), vec![id]);
    assert_eq!(d.app.pin_ids(true), vec![id]);
}

#[test]
fn writingdeck_pins_dashboard_cards_are_real_scoped_readers_with_independent_mouse_scroll() {
    use grimoire_core::pins::Scope;
    let (mut d, note) = secondary_book("pins-dashboard", 100, 30, true);
    let id = pin_document(&mut d, &note);
    d.app
        .pins
        .update(|s| {
            s.pin_mut(id)?.scope = Scope::Dashboard;
            Ok(())
        })
        .unwrap();
    let position = d.app.editor.text();
    d.app.open_dashboard();
    d.draw();
    assert!(d.shows("Secondary paragraph 0."));
    assert_eq!(d.app.pins.dashboard_hits.borrow().len(), 1);
    let rect = d.app.pins.dashboard_hits.borrow()[0].1;
    d.app.on_scroll(rect.x + 1, rect.y + 2, true);
    d.draw();
    assert_eq!(d.app.pins.state().pins[0].scroll, 3);
    assert_eq!(d.app.editor.text(), position);
    d.app.on_click(rect.x + 1, rect.y + 1);
    d.draw();
    assert_eq!(d.app.overlay, Overlay::Pins);
    assert_eq!(d.app.pins.state().selected, Some(id));
}

#[test]
fn writingdeck_pins_sidebar_list_scroll_is_distinct_from_per_card_scroll() {
    let (mut d, note) = secondary_book("pins-list-scroll", 160, 24, true);
    for _ in 0..9 {
        pin_document(&mut d, &note);
    }
    d.key(KeyCode::Esc);
    d.draw();
    let rect = d.app.pins.hits[0].1;
    d.app.on_click(rect.x + 1, rect.y + 1);
    d.draw();
    d.key(KeyCode::PageDown);
    let first = d.app.pins.state().selected.unwrap();
    for _ in 0..7 {
        d.key(KeyCode::Down);
    }
    assert!(d.app.pins.state().sidebar_scroll > 0);
    assert_eq!(
        d.app
            .pins
            .state()
            .pins
            .iter()
            .find(|p| p.id == first)
            .unwrap()
            .scroll,
        3
    );
    let last = d.app.pins.state().selected.unwrap();
    assert_ne!(last, first);
    assert_eq!(
        d.app
            .pins
            .state()
            .pins
            .iter()
            .find(|p| p.id == last)
            .unwrap()
            .scroll,
        0
    );
}

#[test]
fn writingdeck_pins_render_uses_cached_body_and_cannot_recreate_deleted_metadata() {
    use grimoire_core::pins::Store;
    let (mut d, note) = secondary_book("pins-cached-render", 120, 40, false);
    let id = pin_document(&mut d, &note);
    let before = d.app.pins.state().clone();
    fs::write(&note, "Disk-only changed words.\n").unwrap();
    let path = Store::path(&d.root);
    fs::remove_file(&path).unwrap();
    d.draw();
    assert!(d.shows("Secondary beta lives here."));
    assert!(!path.exists());
    assert_eq!(d.app.pins.state(), &before);
    assert!(!d.app.pin_body(id).unwrap().contains("Disk-only"));
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    assert!(d.shows("Disk-only changed words."));
    assert!(!path.exists());
}

#[test]
fn writingdeck_pins_source_shrink_is_visible_and_next_scroll_clamps_without_editing_source() {
    let (mut d, note) = secondary_book("pins-shrink", 120, 40, true);
    pin_document(&mut d, &note);
    for _ in 0..10 {
        d.key(KeyCode::PageDown);
    }
    fs::write(&note, "Short source.\n").unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    assert!(d.shows("Source shortened"));
    d.key(KeyCode::PageUp);
    assert_eq!(d.app.pins.state().pins[0].scroll, 0);
    assert!(d.shows("Short source."));
    assert_eq!(fs::read_to_string(&note).unwrap(), "Short source.\n");
}

#[test]
fn writingdeck_pins_failed_manuscript_save_blocks_source_replacement_and_keeps_recovery() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("pins-source-save-fail", 120, 40, false);
    let third = d.root.join("notes/third-pin-source.md");
    fs::write(&third, "Third source original.\n").unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    choose_secondary(&mut d, &note);
    d.typed("Keep unsaved owner. ");
    let owner = d.app.view_open(Pane::Secondary);
    let backup = note.with_extension("kept");
    fs::rename(&note, &backup).unwrap();
    fs::create_dir(&note).unwrap();
    pin_document(&mut d, &third);
    d.key(KeyCode::Enter);
    assert_eq!(d.app.view_open(Pane::Secondary), owner);
    assert!(
        d.app
            .view_editor(Pane::Secondary)
            .unwrap()
            .text()
            .contains("Keep unsaved owner.")
    );
    assert_eq!(
        fs::read_to_string(&third).unwrap(),
        "Third source original.\n"
    );
    let kept = recovery::dir(&d.root).join(note.strip_prefix(&d.root).unwrap());
    assert!(
        fs::read_to_string(&kept)
            .unwrap()
            .contains("Keep unsaved owner.")
    );
    let primary = d.app.view_open(Pane::Primary);
    d.key(KeyCode::Char('e'));
    assert_eq!(d.app.view_open(Pane::Primary), primary);
    assert!(
        fs::read_to_string(&kept)
            .unwrap()
            .contains("Keep unsaved owner.")
    );
    fs::remove_dir(&note).unwrap();
    fs::rename(&backup, &note).unwrap();
    assert!(d.app.commit_saves());
}

#[test]
fn writingdeck_pins_lessons_require_actual_demo_pin_save_and_anchored_scroll_return() {
    use crate::app::writingdeck::Page;
    let root = std::env::temp_dir().join(format!(
        "grimoire-ui-pins-shipped-tutorial-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    grimoire_core::tutorial::create_demo(&root).unwrap();
    let note = root.join("notes/pin-practice.md");
    let original = fs::read(&note).unwrap();
    let mut d = Desk::open(root, 120, 40);
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 11 });
    d.key(KeyCode::Enter);
    assert!(matches!(d.app.overlay, Overlay::Palette { .. }));
    assert!(!d.app.deck.progress.completed.contains(&11));
    d.typed("Pin Practice");
    d.key(KeyCode::Enter);
    assert!(d.app.deck.progress.completed.contains(&11));
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 12 });
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Pins);
    d.key(KeyCode::Home);
    assert!(!d.app.deck.progress.completed.contains(&12));
    d.key(KeyCode::Char('h'));
    d.key(KeyCode::Home);
    assert!(!d.app.deck.progress.completed.contains(&12));
    d.key(KeyCode::PageDown);
    d.key(KeyCode::Home);
    assert!(d.app.deck.progress.completed.contains(&12));
    let progress = grimoire_core::tutorial::Store::load(&d.root)
        .unwrap()
        .progress;
    assert!(progress.completed.contains(&11));
    assert!(progress.completed.contains(&12));
    assert_eq!(fs::read(&note).unwrap(), original);
    assert_eq!(
        d.app.pins.state().pins[0].source,
        PathBuf::from("notes/pin-practice.md")
    );
}

#[test]
fn writingdeck_pins_tutorial_never_uses_a_real_novel_as_an_exercise() {
    use crate::app::writingdeck::Page;
    let (mut d, note) = secondary_book("pins-tutorial-real", 120, 40, true);
    let original = fs::read(&note).unwrap();
    for id in [11, 12] {
        d.app.overlay = Overlay::Deck(Page::Lesson { id });
        d.key(KeyCode::Enter);
        assert!(d.app.deck.note.contains("safe demo"));
        assert!(!d.app.deck.progress.completed.contains(&id));
        assert!(d.app.deck.progress.active.is_none());
    }
    assert!(d.app.pins.state().pins.is_empty());
    assert_eq!(fs::read(&note).unwrap(), original);
}

#[test]
fn writingdeck_pins_old_layouts_gain_only_the_additive_dashboard_widget() {
    use grimoire_core::workspaces::{Store, Widget};
    let root = book("pins-old-layout", true);
    let mut store = Store::load(&root).unwrap();
    store.state.dashboard.retain(|w| w.widget != Widget::Pins);
    store.state.dashboard[0].visible = false;
    let old = store.state.workspaces.clone();
    store.save().unwrap();
    let d = Desk::open(root, 120, 40);
    assert_eq!(d.app.deck.state.workspaces, old);
    assert!(!d.app.deck.state.dashboard[0].visible);
    assert_eq!(
        d.app
            .deck
            .state
            .dashboard
            .iter()
            .filter(|w| w.widget == Widget::Pins)
            .count(),
        1
    );
}

#[test]
fn writingdeck_pins_custom_alias_opens_board_without_typing_into_prose() {
    use crate::app::writingdeck::Page;
    let (mut d, _) = secondary_book("pins-shortcut", 120, 40, false);
    let original = d.app.editor.text();
    d.app.overlay = Overlay::Deck(Page::Shortcuts {
        sel: 9,
        capture: true,
    });
    d.app.add_deck_shortcut("Ctrl+Alt+P");
    d.app.overlay = Overlay::None;
    d.press(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ));
    assert_eq!(d.app.overlay, Overlay::Pins);
    assert_eq!(d.app.editor.text(), original);
    d.key(KeyCode::F(1));
    assert!(matches!(d.app.overlay, Overlay::Help { .. }));
    assert!(d.shows("Sticky-note pins"));
}

#[test]
fn writingdeck_pins_tab_reveals_the_selected_card_instead_of_focusing_offscreen_content() {
    let (mut d, note) = secondary_book("pins-focus-reveal", 160, 24, true);
    for _ in 0..9 {
        pin_document(&mut d, &note);
    }
    let id = d.app.pins.state().selected.unwrap();
    d.key(KeyCode::Esc);
    assert_eq!(d.app.focus, Focus::Editor);
    d.key(KeyCode::Tab);
    assert_eq!(d.app.focus, Focus::Pins);
    assert!(d.app.pins.hits.iter().any(|(shown, _)| *shown == id));
    d.key(KeyCode::PageDown);
    assert_eq!(
        d.app
            .pins
            .state()
            .pins
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .scroll,
        3
    );
}

#[test]
fn writingdeck_pins_order_skips_archived_cards_and_empty_collection_wheel_is_inert() {
    use grimoire_core::pins::Life;
    let (mut d, note) = secondary_book("pins-filter-order", 120, 40, true);
    let a = pin_document(&mut d, &note);
    let b = pin_document(&mut d, &note);
    let c = pin_document(&mut d, &note);
    d.app
        .pins
        .update(|s| {
            s.set_life(b, Life::Archived)?;
            s.selected = Some(a);
            Ok(())
        })
        .unwrap();
    d.key(KeyCode::Char('J'));
    assert_eq!(
        d.app
            .pins
            .state()
            .pins
            .iter()
            .filter(|p| p.life == Life::Active)
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        vec![c, a]
    );
    d.key(KeyCode::Tab);
    d.key(KeyCode::Char('u'));
    assert_eq!(d.app.pins.board_life, Life::Archived);
    let state = d.app.pins.state().clone();
    let rect = d.app.pins.board_preview.get();
    d.app.on_scroll(rect.x + 1, rect.y + 1, true);
    d.draw();
    assert_eq!(d.app.pins.state(), &state);
}

#[test]
fn writingdeck_pins_external_source_conflict_stays_on_original_while_local_words_are_parked() {
    let (mut d, note) = secondary_book("pins-source-conflict", 120, 40, false);
    let id = pin_document(&mut d, &note);
    d.key(KeyCode::Enter);
    d.app.clip = "Local conflicting source words. ".into();
    d.ctrl('v');
    fs::write(&note, "External source version wins on disk.\n").unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    assert_eq!(
        fs::read_to_string(&note).unwrap(),
        "External source version wins on disk.\n"
    );
    assert!(
        d.app
            .pin_body(id)
            .unwrap()
            .contains("External source version")
    );
    assert!(!d.app.pin_body(id).unwrap().contains("Local conflicting"));
    assert!(
        d.app
            .project
            .nodes
            .iter()
            .any(|n| n.parked && n.body.contains("Local conflicting source words"))
    );
    assert_eq!(d.root.join(&d.app.pins.state().pins[0].source), note);
}

#[test]
fn writingdeck_pins_non_utf8_source_stays_unavailable_without_destroying_its_bytes() {
    let (mut d, note) = secondary_book("pins-source-nonutf8", 120, 40, false);
    let id = pin_document(&mut d, &note);
    let bytes = b"Raw \xff words remain.\n";
    fs::write(&note, bytes).unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.draw();
    assert!(d.app.pin_body(id).is_err());
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Pins);
    assert_eq!(fs::read(&note).unwrap(), bytes);
    assert_eq!(d.app.pins.state().pins.len(), 1);
}

#[test]
fn writingdeck_pins_strict_focus_does_not_focus_a_hidden_secondary_source_owner() {
    use grimoire_core::documents::Pane;
    let (mut d, note) = secondary_book("pins-focus-owner", 160, 50, false);
    choose_secondary(&mut d, &note);
    let selected = d.app.view_position(Pane::Secondary).unwrap();
    pin_document(&mut d, &note);
    d.key(KeyCode::Esc);
    d.ctrl('d');
    assert!(d.app.focus_mode);
    d.app.open_pinboard();
    d.key(KeyCode::Char('e'));
    assert_eq!(d.app.overlay, Overlay::Pins);
    assert_eq!(d.app.focus, Focus::Editor);
    assert_eq!(d.app.view_position(Pane::Secondary).unwrap(), selected);
    assert!(d.app.msg.contains("strict focus"));
}

#[cfg(unix)]
#[test]
fn writingdeck_pins_ambiguous_retained_editor_ownership_refuses_to_guess_a_body() {
    use grimoire_core::documents::Pane;
    use std::os::unix::fs::symlink;
    let (mut d, note) = secondary_book("pins-ambiguous-owner", 160, 50, false);
    choose_secondary(&mut d, &note);
    d.typed("Unique retained local note. ");
    let id = pin_document(&mut d, &note);
    let primary = first_scene(&d.root);
    let original = fs::read(&primary).unwrap();
    let primary_pos = d.app.view_position(Pane::Primary).unwrap();
    let secondary_pos = d.app.view_position(Pane::Secondary).unwrap();
    fs::remove_file(&note).unwrap();
    symlink(&primary, &note).unwrap();
    d.app.refresh_pin_sources();
    assert!(
        d.app
            .pin_body(id)
            .unwrap_err()
            .contains("two retained editor")
    );
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Pins);
    assert_eq!(d.app.view_position(Pane::Primary).unwrap(), primary_pos);
    assert_eq!(d.app.view_position(Pane::Secondary).unwrap(), secondary_pos);
    assert_eq!(fs::read(&primary).unwrap(), original);
    let kept = recovery::dir(&d.root).join(note.strip_prefix(&d.root).unwrap());
    assert!(
        fs::read_to_string(kept)
            .unwrap()
            .contains("Unique retained local note.")
    );
}

fn journal_day(d: &Desk) -> grimoire_core::journal::Day {
    grimoire_core::journal::Store::load(&d.root, &d.app.journal.date)
        .unwrap()
        .day()
        .clone()
}
fn journal_path(d: &Desk) -> PathBuf {
    grimoire_core::journal::Store::path(&d.root, &d.app.journal.date).unwrap()
}
fn reflection_path(d: &Desk) -> PathBuf {
    grimoire_core::journal::reflection_path(&d.root, &d.app.journal.date).unwrap()
}
fn journal_manuscript_save(d: &mut Desk, text: &str) {
    d.app.overlay = Overlay::None;
    d.app
        .run_action(palette::Action::Open(first_scene(&d.root)));
    d.app.paste(text);
    d.ctrl('s');
    d.draw();
}

#[test]
fn writingdeck_journal_saved_facts_and_original_reflection_work_at_four_sizes_and_restart() {
    use grimoire_core::documents::Pane;
    use grimoire_core::journal::Kind as FactKind;
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut d, _) = secondary_book(&format!("journal-flow-{w}"), w, h, false);
        let initial = d.app.project.total_words();
        journal_manuscript_save(&mut d, "Measured manuscript words. ");
        let total = d.app.project.total_words();
        let facts = journal_day(&d);
        assert_eq!(facts.facts.len(), 1);
        let o = &facts.facts[0].observation;
        assert_eq!(o.manuscript_words, total as u64);
        assert_eq!(o.initial_manuscript_words, Some(initial as u64));
        assert_eq!(o.delta_since_open(), Some(total as i64 - initial as i64));
        let source = first_scene(&d.root);
        let manuscript = fs::read(&source).unwrap();
        let recorded = fs::read(journal_path(&d)).unwrap();
        d.app.run_action(palette::Action::Journal);
        d.draw();
        assert!(d.shows("Writing journal"));
        d.key(KeyCode::Tab);
        assert!(d.app.journal.reflection);
        let note = reflection_path(&d);
        assert!(!note.exists());
        d.key(KeyCode::Enter);
        assert_eq!(d.app.focus, Focus::Secondary);
        assert_eq!(
            d.app.project.nodes[d.app.view_open(Pane::Secondary).unwrap()].path,
            note
        );
        d.app.paste("I discovered a quieter ending. ");
        d.ctrl('s');
        assert!(
            fs::read_to_string(&note)
                .unwrap()
                .contains("quieter ending")
        );
        assert_eq!(fs::read(&source).unwrap(), manuscript);
        assert_eq!(fs::read(journal_path(&d)).unwrap(), recorded);
        d.app.run_action(palette::Action::Journal);
        d.key(KeyCode::Tab);
        assert!(d.shows("quieter ending"));
        assert!(d.app.try_quit());
        assert_eq!(
            journal_day(&d).facts.last().unwrap().kind,
            FactKind::ExitRequested
        );
        let final_facts = fs::read(journal_path(&d)).unwrap();
        let note_bytes = fs::read(&note).unwrap();
        let mut restarted = Desk::open(d.root.clone(), w, h);
        restarted.app.run_action(palette::Action::Journal);
        assert_eq!(journal_day(&restarted).facts.len(), 2);
        restarted.key(KeyCode::Tab);
        assert!(
            restarted
                .app
                .journal_body()
                .unwrap()
                .contains("quieter ending")
        );
        assert!(restarted.app.try_quit());
        assert_eq!(fs::read(journal_path(&restarted)).unwrap(), final_facts);
        assert_eq!(fs::read(&note).unwrap(), note_bytes);
    }
}
#[test]
fn writingdeck_journal_view_navigation_and_note_only_saves_never_invent_writing_facts() {
    let (mut d, _) = secondary_book("journal-no-phantoms", 100, 30, false);
    let file = journal_path(&d);
    d.app.run_action(palette::Action::Journal);
    for key in [
        KeyCode::Right,
        KeyCode::Left,
        KeyCode::Char('t'),
        KeyCode::Tab,
        KeyCode::Home,
        KeyCode::Char('r'),
    ] {
        d.key(key);
    }
    assert!(!file.exists());
    d.key(KeyCode::Enter);
    d.app
        .paste("A self-reported reflection, not manuscript writing. ");
    d.ctrl('s');
    assert!(!file.exists());
    assert!(d.app.try_quit());
    assert!(!file.exists());
    assert_eq!(d.app.journal.fact_count(), 0);
}
#[test]
fn writingdeck_journal_negative_net_is_not_gross_deleted_or_active_time() {
    let (mut d, _) = secondary_book("journal-negative", 100, 30, false);
    let total = d.app.project.total_words();
    d.app
        .activate_editor(grimoire_core::documents::Pane::Primary);
    d.ctrl('a');
    d.ctrl('x');
    d.ctrl('s');
    let day = journal_day(&d);
    let o = &day.facts[0].observation;
    assert!(o.delta_since_open().unwrap() < 0);
    assert_eq!(
        o.delta_since_open(),
        Some(d.app.project.total_words() as i64 - total as i64)
    );
    assert!(o.daily_net_words.unwrap() < 0);
    let text = fs::read_to_string(journal_path(&d)).unwrap();
    for invented in ["active_seconds", "gross_words", "deleted_words", "duration"] {
        assert!(!text.contains(invented));
    }
}
#[test]
fn writingdeck_journal_checkpoints_are_throttled_and_exit_request_is_idempotent() {
    let (mut d, _) = secondary_book("journal-throttle", 100, 30, false);
    journal_manuscript_save(&mut d, "First real save. ");
    assert_eq!(journal_day(&d).facts.len(), 1);
    journal_manuscript_save(&mut d, "Another real save. ");
    assert_eq!(journal_day(&d).facts.len(), 1);
    d.app.journal.expire_checkpoint_for_test();
    journal_manuscript_save(&mut d, "After the monotonic interval. ");
    assert_eq!(journal_day(&d).facts.len(), 2);
    assert!(d.app.try_quit());
    let bytes = fs::read(journal_path(&d)).unwrap();
    assert!(d.app.try_quit());
    assert_eq!(fs::read(journal_path(&d)).unwrap(), bytes);
    assert_eq!(journal_day(&d).facts.len(), 3);
}
#[test]
fn writingdeck_journal_existing_source_yaml_undo_and_primary_ownership_are_preserved() {
    use grimoire_core::documents::Pane;
    let (mut d, research) = secondary_book("journal-owner", 160, 50, false);
    let path = reflection_path(&d);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = "---\ntitle: My own reflection\ncustom:\n  values: [one, two] # untouched\n---\n\nOriginal reflection.\n";
    fs::write(&path, original).unwrap();
    d.app.reload_for_shots();
    choose_secondary(&mut d, &research);
    d.app.run_action(palette::Action::Open(path.clone()));
    d.app.paste("Unique unsaved reflection. ");
    let body = d.app.editor.text();
    let position = d.app.view_position(Pane::Primary).unwrap();
    let second = d.app.view_position(Pane::Secondary).unwrap();
    d.app.run_action(palette::Action::Journal);
    assert!(d.app.journal_body().unwrap().contains("Unique unsaved"));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.focus, Focus::Editor);
    assert_eq!(d.app.view_position(Pane::Primary).unwrap(), position);
    assert_eq!(d.app.view_position(Pane::Secondary).unwrap(), second);
    assert_eq!(d.app.editor.text(), body);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    d.ctrl('s');
    let (front, _) = project::split_frontmatter(&fs::read_to_string(&path).unwrap());
    let (expected, _) = project::split_frontmatter(original);
    assert_eq!(front, expected);
    d.ctrl('z');
    assert!(!d.app.editor.text().contains("Unique unsaved"));
    d.ctrl('y');
    assert!(d.app.editor.text().contains("Unique unsaved"));
}
#[test]
fn writingdeck_journal_readonly_controls_cut_undo_paste_and_drag_cannot_mutate_hidden_editors() {
    use grimoire_core::documents::Pane;
    let (mut d, _) = secondary_book("journal-readonly", 120, 40, true);
    d.app.activate_editor(Pane::Primary);
    d.app.paste("Do not undo this while reading. ");
    d.app.editor.select((0, 0), (0, 10));
    let body = d.app.editor.text();
    let position = d.app.view_position(Pane::Primary).unwrap();
    d.app.run_action(palette::Action::Journal);
    d.app.clip = "eettrr".into();
    for key in ['z', 'y', 'x', 'a', 'v'] {
        d.ctrl(key);
    }
    d.typed("ZZZZ");
    let r = d.app.rect_editor;
    d.app.on_drag(r.x + 3, r.y + 3);
    assert_eq!(d.app.editor.text(), body);
    assert_eq!(d.app.view_position(Pane::Primary).unwrap(), position);
    assert!(!reflection_path(&d).exists());
    assert_eq!(d.app.overlay, Overlay::Journal);
}
#[test]
fn writingdeck_journal_cached_drawing_performs_no_source_reads_or_metadata_writes() {
    let (mut d, _) = secondary_book("journal-cache", 100, 30, false);
    journal_manuscript_save(&mut d, "Saved. ");
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Enter);
    d.app.paste("Cached reflection survives a missing file. ");
    d.ctrl('s');
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Tab);
    let facts = journal_path(&d);
    let note = reflection_path(&d);
    fs::remove_file(&facts).unwrap();
    fs::remove_file(&note).unwrap();
    for _ in 0..3 {
        d.draw();
        assert!(d.shows("Cached reflection"));
    }
    assert!(!facts.exists());
    assert!(!note.exists());
    d.app.refresh_journal_source();
    assert!(d.app.journal_body().is_err());
}
#[test]
fn writingdeck_journal_fact_and_reflection_reader_scrolls_preserve_both_editor_positions() {
    use grimoire_core::documents::Pane;
    use grimoire_core::journal::{Kind as FactKind, Observation, Store};
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut d, research) = secondary_book(&format!("journal-scroll-{w}"), w, h, true);
        let date = d.app.journal.date.clone();
        let mut store = Store::load(&d.root, &date).unwrap();
        for n in 0..24 {
            let at = chrono::DateTime::parse_from_rfc3339(&format!("{date}T10:{n:02}:00+00:00"))
                .unwrap();
            store
                .append(
                    FactKind::SavedCheckpoint,
                    Observation::new(at, at, Some(10), 10 + n, Some(n as i64)).unwrap(),
                )
                .unwrap();
        }
        let path = reflection_path(&d);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            (0..80)
                .map(|n| format!("Reflection row {n:02}.\n"))
                .collect::<String>(),
        )
        .unwrap();
        d.app.reload_for_shots();
        choose_secondary(&mut d, &research);
        d.app.editor.select((2, 2), (4, 5));
        let secondary = d.app.view_position(Pane::Secondary).unwrap();
        let primary = d.app.view_position(Pane::Primary).unwrap();
        d.app.run_action(palette::Action::Journal);
        d.key(KeyCode::PageDown);
        assert_eq!(d.app.journal.scroll, 10);
        d.key(KeyCode::Tab);
        assert_eq!(d.app.journal.scroll, 0);
        d.key(KeyCode::PageDown);
        assert_eq!(d.app.journal.scroll, 10);
        assert!(d.shows("Reflection row 10"));
        d.key(KeyCode::End);
        d.key(KeyCode::Home);
        assert_eq!(d.app.view_position(Pane::Primary).unwrap(), primary);
        assert_eq!(d.app.view_position(Pane::Secondary).unwrap(), secondary);
    }
}
#[test]
fn writingdeck_journal_unknown_future_and_corrupt_facts_preserve_bytes_and_block_unrecorded_quit() {
    use grimoire_core::journal::{Day, Store};
    for (tag, text) in [
        ("broken", "{ broken".into()),
        ("future", {
            let mut d = Day::empty(&chrono::Local::now().format("%Y-%m-%d").to_string()).unwrap();
            d.version = 999;
            serde_json::to_string(&d).unwrap()
        }),
        (
            "unknown",
            format!(
                "{{\"version\":1,\"date\":\"{}\",\"next_id\":1,\"facts\":[],\"future\":true}}",
                chrono::Local::now().format("%Y-%m-%d")
            ),
        ),
    ] {
        let root = book(&format!("journal-{tag}"), true);
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let file = Store::path(&root, &date).unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, &text).unwrap();
        let mut d = Desk::open(root, 100, 30);
        journal_manuscript_save(&mut d, "The manuscript must still save. ");
        assert!(
            fs::read_to_string(first_scene(&d.root))
                .unwrap()
                .contains("manuscript must still save")
        );
        assert_eq!(d.app.journal.pending.len(), 1);
        assert_eq!(fs::read_to_string(&file).unwrap(), text);
        assert!(!d.app.try_quit());
        assert_eq!(d.app.journal.pending.len(), 2);
        d.app.run_action(palette::Action::Journal);
        d.draw();
        assert!(d.shows("Recorded observations unavailable"));
        d.key(KeyCode::Char('r'));
        assert_eq!(fs::read_to_string(&file).unwrap(), text);
        assert_eq!(d.app.journal.fact_count(), 0);
        assert!(!d.app.journal.pending.is_empty());
    }
}
#[test]
fn writingdeck_journal_external_append_requires_explicit_reload_before_merging_pending_facts() {
    use grimoire_core::journal::{Kind as FactKind, Observation, Store};
    let (mut d, _) = secondary_book("journal-external", 100, 30, false);
    let date = d.app.journal.date.clone();
    let mut external = Store::load(&d.root, &date).unwrap();
    let at = chrono::Local::now().fixed_offset();
    external
        .append(
            FactKind::SavedCheckpoint,
            Observation::new(at, at, Some(13), 17, Some(4)).unwrap(),
        )
        .unwrap();
    let bytes = fs::read(journal_path(&d)).unwrap();
    journal_manuscript_save(&mut d, "Own saved checkpoint. ");
    assert_eq!(d.app.journal.pending.len(), 1);
    assert_eq!(fs::read(journal_path(&d)).unwrap(), bytes);
    d.app.run_action(palette::Action::Journal);
    assert_eq!(d.app.journal.fact_count(), 1);
    assert_eq!(d.app.journal.pending.len(), 1);
    d.key(KeyCode::Char('r'));
    assert!(d.app.journal.pending.is_empty());
    let day = journal_day(&d);
    assert_eq!(day.facts.len(), 2);
    assert_eq!(day.facts[0], external.day().facts[0]);
    assert!(d.app.try_quit());
    assert_eq!(journal_day(&d).facts.len(), 3);
}
#[test]
fn writingdeck_journal_io_failure_keeps_pending_observation_until_explicit_retry() {
    let (mut d, _) = secondary_book("journal-io", 100, 30, false);
    let file = journal_path(&d);
    fs::create_dir_all(&file).unwrap();
    journal_manuscript_save(&mut d, "Save prose despite journal failure. ");
    assert_eq!(d.app.journal.pending.len(), 1);
    assert_eq!(d.app.journal.fact_count(), 0);
    let source = fs::read(first_scene(&d.root)).unwrap();
    fs::remove_dir(&file).unwrap();
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Char('r'));
    assert!(d.app.journal.pending.is_empty());
    assert_eq!(journal_day(&d).facts.len(), 1);
    assert_eq!(fs::read(first_scene(&d.root)).unwrap(), source);
}
#[test]
fn writingdeck_journal_waiting_source_save_cannot_create_reflection_or_record_clean_facts_or_quit()
{
    let (mut d, _) = secondary_book("journal-waiting", 100, 30, false);
    d.app
        .activate_editor(grimoire_core::documents::Pane::Primary);
    d.app.paste("Keep these unsaved manuscript words. ");
    let file = first_scene(&d.root);
    let backup = file.with_extension("kept");
    fs::rename(&file, &backup).unwrap();
    fs::create_dir(&file).unwrap();
    d.ctrl('s');
    assert!(d.app.project.dirty_count() > 0);
    assert!(!journal_path(&d).exists());
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Journal);
    assert!(!reflection_path(&d).exists());
    assert!(!d.app.try_quit());
    assert!(d.app.editor.text().contains("unsaved manuscript"));
    let copy = recovery::dir(&d.root).join(file.strip_prefix(&d.root).unwrap());
    assert!(
        fs::read_to_string(copy)
            .unwrap()
            .contains("unsaved manuscript")
    );
    fs::remove_dir(&file).unwrap();
    fs::rename(&backup, &file).unwrap();
}
#[cfg(unix)]
#[test]
fn writingdeck_journal_reflection_outside_and_manuscript_aliases_are_not_used_as_manual_sources() {
    use std::os::unix::fs::symlink;
    let (mut d, _) = secondary_book("journal-source-containment", 100, 30, false);
    let file = reflection_path(&d);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let outside = d
        .root
        .parent()
        .unwrap()
        .join(format!("wd-journal-outside-{}", std::process::id()));
    fs::write(&outside, "Outside text must not be displayed or changed.").unwrap();
    symlink(&outside, &file).unwrap();
    d.app.reload_for_shots();
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Tab);
    assert!(d.app.journal_body().is_err());
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Journal);
    assert_eq!(
        fs::read_to_string(&outside).unwrap(),
        "Outside text must not be displayed or changed."
    );
    fs::remove_file(&file).unwrap();
    let manuscript = first_scene(&d.root);
    let bytes = fs::read(&manuscript).unwrap();
    symlink(&manuscript, &file).unwrap();
    d.app.refresh_journal_source();
    assert!(d.app.journal_body().unwrap_err().contains("manuscript"));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Journal);
    assert_eq!(fs::read(&manuscript).unwrap(), bytes);
    fs::remove_file(outside).unwrap();
}
#[cfg(unix)]
#[test]
fn writingdeck_journal_non_utf8_reflection_is_unavailable_and_never_replaced() {
    let (mut d, _) = secondary_book("journal-nonutf8", 100, 30, false);
    let file = reflection_path(&d);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let bytes = b"\xff\xfe original bytes";
    fs::write(&file, bytes).unwrap();
    d.app.reload_for_shots();
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Tab);
    assert!(d.app.journal_body().is_err());
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Journal);
    assert_eq!(fs::read(file).unwrap(), bytes);
}
#[test]
fn writingdeck_journal_strict_focus_cannot_create_or_focus_a_hidden_secondary_reflection() {
    use grimoire_core::documents::Pane;
    let (mut d, _) = secondary_book("journal-strict", 120, 40, false);
    d.app.activate_editor(Pane::Primary);
    d.ctrl('d');
    assert!(d.app.focus_mode);
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Enter);
    assert!(!reflection_path(&d).exists());
    assert_eq!(d.app.overlay, Overlay::Journal);
    d.key(KeyCode::Esc);
    d.ctrl('d');
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Enter);
    assert_eq!(d.app.focus, Focus::Secondary);
    d.app.activate_editor(Pane::Primary);
    d.ctrl('d');
    let primary = d.app.view_position(Pane::Primary).unwrap();
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Char('e'));
    assert_eq!(d.app.overlay, Overlay::Journal);
    assert_eq!(d.app.view_position(Pane::Primary).unwrap(), primary);
    assert_eq!(d.app.views.active, Pane::Primary);
}
#[test]
fn writingdeck_journal_tutorial_requires_recorded_facts_review_and_actual_saved_reflection() {
    use crate::app::writingdeck::Page;
    let root = std::env::temp_dir().join(format!(
        "grimoire-ui-journal-shipped-demo-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    grimoire_core::tutorial::create_demo(&root).unwrap();
    let mut d = Desk::open(root, 120, 40);
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 14 });
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Journal);
    assert!(!d.app.deck.progress.completed.contains(&14));
    d.key(KeyCode::Tab);
    d.key(KeyCode::Enter);
    d.app
        .paste("Reflection without a reviewed recorded checkpoint. ");
    d.ctrl('s');
    assert!(!d.app.deck.progress.completed.contains(&14));
    let first = d
        .app
        .project
        .nodes
        .iter()
        .find(|n| n.in_manuscript && n.kind == project::Kind::Scene)
        .unwrap()
        .path
        .clone();
    d.app.run_action(palette::Action::Open(first));
    d.app.paste("Real manuscript work in the safe demo. ");
    d.ctrl('s');
    d.app.run_action(palette::Action::Journal);
    assert!(!d.app.journal.day.facts.is_empty());
    d.key(KeyCode::Tab);
    d.key(KeyCode::Enter);
    assert!(!d.app.deck.progress.completed.contains(&14));
    d.app.paste("I reflected after seeing the saved facts. ");
    assert!(!d.app.deck.progress.completed.contains(&14));
    d.ctrl('s');
    assert!(d.app.deck.progress.completed.contains(&14));
    assert!(
        grimoire_core::tutorial::Store::load(&d.root)
            .unwrap()
            .progress
            .completed
            .contains(&14)
    );
}
#[test]
fn writingdeck_journal_lesson_never_uses_an_ordinary_novel_for_its_exercise() {
    use crate::app::writingdeck::Page;
    let (mut d, _) = secondary_book("journal-real-novel", 100, 30, false);
    let before = fs::read(first_scene(&d.root)).unwrap();
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 14 });
    d.key(KeyCode::Enter);
    assert!(d.app.deck.note.contains("safe demo"));
    assert!(d.app.deck.progress.active.is_none());
    assert!(!journal_path(&d).exists());
    assert!(!reflection_path(&d).exists());
    assert_eq!(fs::read(first_scene(&d.root)).unwrap(), before);
}
#[test]
fn writingdeck_journal_widget_alias_help_and_old_layouts_are_additive_without_default_rebinding() {
    use crate::app::writingdeck::Page;
    use grimoire_core::workspaces::{Store, Widget};
    let root = book("journal-layout-alias", true);
    let mut store = Store::load(&root).unwrap();
    store
        .state
        .dashboard
        .retain(|w| w.widget != Widget::Journal);
    store.state.dashboard[0].visible = false;
    let old = store.state.workspaces.clone();
    let old_widgets = store.state.dashboard.clone();
    store.save().unwrap();
    let mut d = Desk::open(root, 120, 40);
    assert_eq!(d.app.deck.state.workspaces, old);
    assert_eq!(d.app.deck.state.dashboard[..old_widgets.len()], old_widgets);
    assert_eq!(
        d.app.deck.state.dashboard.last().unwrap().widget,
        Widget::Journal
    );
    let before = d.app.editor.text();
    d.app.overlay = Overlay::Deck(Page::Shortcuts {
        sel: 12,
        capture: true,
    });
    d.app.add_deck_shortcut("Ctrl+Alt+J");
    d.app.overlay = Overlay::None;
    d.press(KeyEvent::new(
        KeyCode::Char('j'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ));
    assert_eq!(d.app.overlay, Overlay::Journal);
    assert_eq!(d.app.editor.text(), before);
    d.key(KeyCode::F(1));
    assert!(matches!(d.app.overlay, Overlay::Help { .. }));
    assert!(d.shows("Writing journal"));
    d.key(KeyCode::Esc);
    d.app.open_dashboard();
    d.key(KeyCode::Char('j'));
    assert_eq!(d.app.overlay, Overlay::Journal);
}
#[test]
fn writingdeck_journal_selected_date_controls_only_its_own_reflection_source() {
    let (mut d, _) = secondary_book("journal-date", 100, 30, false);
    let today = d.app.journal.date.clone();
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Left);
    let previous = d.app.journal.date.clone();
    assert_ne!(previous, today);
    d.key(KeyCode::Enter);
    d.app.paste("A reflection for yesterday. ");
    d.ctrl('s');
    let old = grimoire_core::journal::reflection_path(&d.root, &previous).unwrap();
    assert!(fs::read_to_string(old).unwrap().contains("for yesterday"));
    assert!(
        !grimoire_core::journal::reflection_path(&d.root, &today)
            .unwrap()
            .exists()
    );
    assert!(!journal_path(&d).exists());
    d.app.run_action(palette::Action::Journal);
    assert_eq!(d.app.journal.date, today);
    d.key(KeyCode::Tab);
    assert!(d.app.journal_body().is_err());
}
#[test]
fn writingdeck_journal_book_switch_resets_observation_baseline_and_never_leaks_other_reflections() {
    let (mut d, _) = secondary_book("journal-book-a", 100, 30, false);
    journal_manuscript_save(&mut d, "A book private writing. ");
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Enter);
    d.app.paste("Private reflection in book A. ");
    d.ctrl('s');
    let other = Desk::open(book("journal-book-b", true), 100, 30);
    let initial = other.app.project.total_words();
    assert!(d.app.open_book(&other.root));
    assert_eq!(d.app.journal.fact_count(), 0);
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Tab);
    assert!(
        !d.app
            .journal_body()
            .is_ok_and(|b| b.contains("Private reflection"))
    );
    d.app.overlay = Overlay::None;
    let first = first_scene(&other.root);
    d.app.run_action(palette::Action::Open(first));
    d.app.paste("Different book words. ");
    d.ctrl('s');
    let day = grimoire_core::journal::Store::load(&other.root, &d.app.journal.date).unwrap();
    assert_eq!(
        day.day().facts[0].observation.initial_manuscript_words,
        Some(initial as u64)
    );
}
#[cfg(unix)]
#[test]
fn writingdeck_journal_ambiguous_retained_editor_aliases_refuse_to_guess_and_rescue_both_bodies() {
    use grimoire_core::documents::Pane;
    use std::os::unix::fs::symlink;
    let (mut d, research) = secondary_book("journal-ambiguous", 160, 50, false);
    let note = grimoire_core::journal::ensure_reflection(&d.root, &d.app.journal.date).unwrap();
    d.app.reload_for_shots();
    choose_secondary(&mut d, &research);
    d.app.run_action(palette::Action::Open(note.clone()));
    d.app.paste("Unique unsaved reflection context. ");
    d.app.activate_editor(Pane::Secondary);
    d.app.paste("Unique unsaved research context. ");
    let original = fs::read(&note).unwrap();
    fs::remove_file(&research).unwrap();
    symlink(&note, &research).unwrap();
    d.app.run_action(palette::Action::Journal);
    assert!(d.app.journal_body().unwrap_err().contains("two retained"));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Journal);
    assert_eq!(fs::read(&note).unwrap(), original);
    for (path, words) in [
        (&note, "unsaved reflection"),
        (&research, "unsaved research"),
    ] {
        let copy = recovery::dir(&d.root).join(path.strip_prefix(&d.root).unwrap());
        assert!(fs::read_to_string(copy).unwrap().contains(words));
    }
}

#[test]
fn writingdeck_journal_explicit_snapshot_requires_real_saved_work_and_never_duplicates_unchanged_counts()
 {
    let (mut d, _) = secondary_book("journal-explicit", 100, 30, false);
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Char('c'));
    assert!(!journal_path(&d).exists());
    journal_manuscript_save(&mut d, "First checkpoint. ");
    journal_manuscript_save(&mut d, "Updated saved project counts. ");
    assert_eq!(journal_day(&d).facts.len(), 1);
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Char('c'));
    assert_eq!(journal_day(&d).facts.len(), 2);
    d.key(KeyCode::Char('c'));
    assert_eq!(journal_day(&d).facts.len(), 2);
    d.key(KeyCode::Esc);
    d.app.paste("Not saved yet. ");
    d.app.run_action(palette::Action::Journal);
    d.key(KeyCode::Char('c'));
    assert_eq!(journal_day(&d).facts.len(), 2);
    assert!(d.app.msg.contains("Save both"));
}

#[test]
fn writingdeck_journal_unreadable_manuscript_totals_do_not_become_invented_complete_counts() {
    let root = book("journal-incomplete", true);
    let source = first_scene(&root);
    let unknown = source.with_file_name("02-Unreadable.md");
    fs::write(&unknown, b"\xff\xfe must not become zero words").unwrap();
    let mut d = Desk::open(root, 100, 30);
    assert!(
        d.app
            .project
            .nodes
            .iter()
            .any(|n| n.in_manuscript && n.read_only)
    );
    journal_manuscript_save(&mut d, "Known source writing still saves. ");
    assert!(
        fs::read_to_string(&source)
            .unwrap()
            .contains("Known source writing")
    );
    assert!(!journal_path(&d).exists());
    assert!(d.app.journal.warning.contains("total unavailable"));
    assert!(!d.app.try_quit());
    assert!(d.app.journal.pending.is_empty());
    d.app.overlay = Overlay::Journal;
    d.app.msg = "Ctrl-Q again to quit anyway · Journal exit observation unavailable".into();
    d.draw();
    assert!(d.shows("Q again to quit anyway"));
    d.app.overlay = Overlay::None;
    assert!(!journal_path(&d).exists());
    assert_eq!(
        fs::read(&unknown).unwrap(),
        b"\xff\xfe must not become zero words"
    );
    fs::write(&unknown, "Repaired external source.\n").unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    journal_manuscript_save(&mut d, "After repair. ");
    let day = journal_day(&d);
    assert_eq!(day.facts.len(), 1);
    let o = &day.facts[0].observation;
    assert_eq!(o.manuscript_words, d.app.project.total_words() as u64);
    assert!(o.initial_manuscript_words.is_none());
    assert!(o.delta_since_open().is_none());
    assert!(o.daily_net_words.is_none());
}

// ---- M4: source-preserving scratchpad, handoff and explicit pane starters ----
fn workflow_note(d: &mut Desk, body: &str) -> (u64, PathBuf) {
    d.app.run_action(palette::Action::Scratchpad);
    d.key(KeyCode::Char('n'));
    assert_eq!(d.app.overlay, Overlay::None, "{}", d.app.msg);
    assert_eq!(d.app.focus, Focus::Secondary);
    let id = d.app.workflow.selected.unwrap();
    let path = d.root.join(grimoire_core::workflow::scratch_path(id));
    d.app.paste(body);
    d.ctrl('s');
    assert!(fs::read_to_string(&path).unwrap().contains(body));
    (id, path)
}
fn workflow_link(d: &mut Desk, id: u64) {
    let scene = first_scene(&d.root);
    d.app.link_scratchpad(id, &scene);
    assert_eq!(
        d.app
            .workflow
            .state()
            .unwrap()
            .note(id)
            .unwrap()
            .scene
            .as_deref(),
        Some(scene.strip_prefix(&d.root).unwrap())
    );
}
fn workflow_intention(d: &mut Desk, body: &str) {
    d.app.run_action(palette::Action::Handoff);
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::None, "{}", d.app.msg);
    d.app.paste(body);
    d.ctrl('s');
    d.app.run_action(palette::Action::Handoff);
    d.key(KeyCode::Char('s'));
    assert!(
        d.app.workflow.state().unwrap().handoff.show_on_open,
        "{}",
        d.app.msg
    );
    assert!(d.app.workflow.state().unwrap().handoff.intention.is_some());
}
#[test]
fn writingdeck_workflow_four_size_multi_note_scene_link_and_original_preserving_promotion() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut d, _) = secondary_book(&format!("workflow-sizes-{width}"), width, height, false);
        let manuscript = fs::read(first_scene(&d.root)).unwrap();
        let before = d.app.project.total_words();
        let (id, source) = workflow_note(&mut d, "Original scene-linked scratch idea. ");
        assert_eq!(d.app.project.total_words(), before);
        assert!(!journal_path(&d).exists());
        let original = fs::read(&source).unwrap();
        workflow_link(&mut d, id);
        d.key(KeyCode::Char('p'));
        let target = d.root.join(grimoire_core::workflow::promotion_path(id));
        assert!(target.is_file(), "{}", d.app.msg);
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(fs::read(first_scene(&d.root)).unwrap(), manuscript);
        assert!(d.app.project.total_words() > before);
        assert!(!journal_path(&d).exists());
        let promoted = fs::read(&target).unwrap();
        d.key(KeyCode::Char('p'));
        assert_eq!(fs::read(&target).unwrap(), promoted);
        let (other, other_path) = workflow_note(&mut d, "Separate second note. ");
        assert_ne!(id, other);
        assert_ne!(source, other_path);
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(d.app.workflow.state().unwrap().notes.len(), 2);
        assert!(d.app.try_quit());
        let root = d.root.clone();
        let mut restarted = Desk::open(root, width, height);
        restarted.app.run_action(palette::Action::Scratchpad);
        assert_eq!(restarted.app.workflow.state().unwrap().notes.len(), 2);
        restarted.app.workflow.selected = Some(id);
        restarted.key(KeyCode::Enter);
        restarted.app.paste("Loaded original after restart. ");
        restarted.ctrl('s');
        assert!(
            fs::read_to_string(&source)
                .unwrap()
                .contains("Loaded original after restart")
        );
        assert_eq!(fs::read(target).unwrap(), promoted);
    }
}
#[test]
fn writingdeck_workflow_existing_owner_raw_yaml_and_undo_remain_original() {
    use grimoire_core::documents::Pane;
    let (mut d, _) = secondary_book("workflow-owner", 120, 40, false);
    let (id, path) = workflow_note(&mut d, "First original. ");
    d.app.paste("Second original edit. ");
    d.ctrl('s');
    let edited = fs::read(&path).unwrap();
    d.app.run_action(palette::Action::Scratchpad);
    d.key(KeyCode::Char('e'));
    assert_eq!(d.app.views.active, Pane::Secondary);
    assert_eq!(fs::read(&path).unwrap(), edited);
    d.ctrl('z');
    d.ctrl('s');
    assert!(
        !fs::read_to_string(&path)
            .unwrap()
            .contains("Second original edit")
    );
    d.ctrl('y');
    d.ctrl('s');
    assert_eq!(fs::read(&path).unwrap(), edited);
    d.app.run_action(palette::Action::Scratchpad);
    workflow_link(&mut d, id);
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('v'));
    assert_eq!(d.app.views.active, Pane::Primary);
    assert_eq!(
        d.app.project.nodes[d.app.open.unwrap()].path,
        d.root.join(grimoire_core::workflow::promotion_path(id))
    );
    assert_eq!(fs::read(path).unwrap(), edited);
}
#[test]
fn writingdeck_workflow_reader_scroll_cut_and_undo_never_follow_or_mutate_either_editor() {
    use grimoire_core::documents::Pane;
    let (mut d, _) = secondary_book("workflow-reader", 100, 30, true);
    let (_, path) = workflow_note(
        &mut d,
        &(0..70)
            .map(|i| format!("Original scratch paragraph {i}.\n"))
            .collect::<String>(),
    );
    let source = fs::read(&path).unwrap();
    let a = d.app.view_position(Pane::Primary);
    let b = d.app.view_position(Pane::Secondary);
    d.app.run_action(palette::Action::Scratchpad);
    d.key(KeyCode::End);
    assert!(d.app.workflow.scroll > 0);
    d.ctrl('z');
    d.ctrl('x');
    d.ctrl('v');
    assert_eq!(d.app.view_position(Pane::Primary), a);
    assert_eq!(d.app.view_position(Pane::Secondary), b);
    assert_eq!(fs::read(path).unwrap(), source);
    d.key(KeyCode::Home);
    assert_eq!(d.app.workflow.scroll, 0);
}
#[test]
fn writingdeck_workflow_render_is_cache_only_even_when_originals_and_index_disappear() {
    let (mut d, _) = secondary_book("workflow-cache", 100, 30, false);
    let (_, path) = workflow_note(&mut d, "Cached original marker. ");
    d.app.run_action(palette::Action::Scratchpad);
    fs::remove_file(path).unwrap();
    let meta = grimoire_core::workflow::Store::path(&d.root);
    let raw = fs::read(&meta).unwrap();
    fs::remove_file(&meta).unwrap();
    d.draw();
    assert!(d.shows("Cached original marker"));
    assert!(!meta.exists());
    fs::write(meta, raw).unwrap();
}
#[test]
fn writingdeck_workflow_missing_original_is_not_recreated_or_promoted() {
    let (mut d, _) = secondary_book("workflow-missing", 100, 30, false);
    let (id, path) = workflow_note(&mut d, "Keep missing source words. ");
    workflow_link(&mut d, id);
    d.app.overlay = Overlay::None;
    d.app
        .activate_editor(grimoire_core::documents::Pane::Primary);
    d.app.hide_secondary();
    fs::remove_file(&path).unwrap();
    d.app.run_action(palette::Action::Scratchpad);
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('p'));
    assert!(!path.exists());
    assert!(
        !d.root
            .join(grimoire_core::workflow::promotion_path(id))
            .exists()
    );
}
#[test]
fn writingdeck_workflow_archive_trash_and_restore_never_delete_original_notes() {
    let (mut d, _) = secondary_book("workflow-life", 100, 30, false);
    let (id, path) = workflow_note(&mut d, "Keep original throughout lifecycle. ");
    let raw = fs::read(&path).unwrap();
    d.app.run_action(palette::Action::Scratchpad);
    d.key(KeyCode::Char('a'));
    assert_eq!(
        d.app.workflow.state().unwrap().note(id).unwrap().life,
        grimoire_core::pins::Life::Archived
    );
    d.key(KeyCode::Tab);
    d.key(KeyCode::Delete);
    assert_eq!(
        d.app.workflow.state().unwrap().note(id).unwrap().life,
        grimoire_core::pins::Life::Trashed
    );
    d.key(KeyCode::Tab);
    d.key(KeyCode::Char('a'));
    assert_eq!(
        d.app.workflow.state().unwrap().note(id).unwrap().life,
        grimoire_core::pins::Life::Active
    );
    assert_eq!(fs::read(path).unwrap(), raw);
}
#[test]
fn writingdeck_workflow_metadata_conflict_and_index_io_failure_roll_back_before_copying() {
    let (mut d, _) = secondary_book("workflow-conflict", 100, 30, false);
    let (id, path) = workflow_note(&mut d, "Do not overwrite in promotion. ");
    workflow_link(&mut d, id);
    let original = fs::read(&path).unwrap();
    let meta = grimoire_core::workflow::Store::path(&d.root);
    let valid = fs::read(&meta).unwrap();
    fs::write(&meta, "External incompatible workflow").unwrap();
    d.key(KeyCode::Char('p'));
    assert!(
        !d.root
            .join(grimoire_core::workflow::promotion_path(id))
            .exists()
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(
        fs::read_to_string(&meta).unwrap(),
        "External incompatible workflow"
    );
    fs::write(&meta, valid).unwrap();
    d.key(KeyCode::Char('r'));
    fs::remove_file(&meta).unwrap();
    fs::create_dir(&meta).unwrap();
    d.key(KeyCode::Char('p'));
    assert!(
        !d.root
            .join(grimoire_core::workflow::promotion_path(id))
            .exists()
    );
    assert_eq!(fs::read(&path).unwrap(), original);
}
#[test]
fn writingdeck_workflow_unknown_future_state_never_creates_scratch_sources_or_rewrites_metadata() {
    for raw in [
        "bad",
        "{\"version\":999}",
        "{\"version\":1,\"future\":true}",
    ] {
        let root = book(&format!("workflow-invalid-{}", raw.len()), true);
        let meta = grimoire_core::workflow::Store::path(&root);
        fs::create_dir_all(meta.parent().unwrap()).unwrap();
        fs::write(&meta, raw).unwrap();
        let mut d = Desk::open(root, 80, 24);
        d.app.run_action(palette::Action::Scratchpad);
        d.draw();
        assert!(d.shows("Workflow metadata unavailable"));
        d.key(KeyCode::Char('n'));
        assert_eq!(fs::read_to_string(&meta).unwrap(), raw);
        assert!(!d.root.join("notes/scratchpad").exists());
        assert_eq!(d.app.overlay, Overlay::Scratchpad);
    }
}
#[test]
fn writingdeck_workflow_waiting_dirty_source_blocks_new_note_handoff_and_starter_without_lost_buffers()
 {
    let (mut d, _) = secondary_book("workflow-waiting", 100, 30, false);
    d.app
        .activate_editor(grimoire_core::documents::Pane::Primary);
    d.app.paste("Unsaved protected primary words. ");
    let path = first_scene(&d.root);
    let backup = path.with_extension("backup");
    fs::rename(&path, &backup).unwrap();
    fs::create_dir(&path).unwrap();
    d.app.run_action(palette::Action::Scratchpad);
    d.key(KeyCode::Char('n'));
    assert!(!d.root.join("notes/scratchpad").exists());
    assert!(d.app.editor.text().contains("Unsaved protected"));
    d.app.run_action(palette::Action::Handoff);
    d.key(KeyCode::Enter);
    assert!(!d.root.join("notes/handoff.md").exists());
    d.app
        .apply_starter_preset(1, grimoire_core::workspaces::Preset::Planning);
    assert!(!d.root.join("notes/workspaces/planning.md").exists());
    assert!(d.app.project.dirty_count() > 0);
    assert!(d.app.editor.text().contains("Unsaved protected"));
    let copy = recovery::dir(&d.root).join(path.strip_prefix(&d.root).unwrap());
    assert!(
        fs::read_to_string(copy)
            .unwrap()
            .contains("Unsaved protected")
    );
    fs::remove_dir(&path).unwrap();
    fs::rename(backup, path).unwrap();
}
#[test]
fn writingdeck_workflow_strict_focus_cannot_create_or_focus_hidden_secondary() {
    let (mut d, _) = secondary_book("workflow-strict", 100, 30, false);
    let (_, source) = workflow_note(&mut d, "Owned secondary note. ");
    d.app
        .activate_editor(grimoire_core::documents::Pane::Primary);
    d.ctrl('d');
    assert!(d.app.focus_mode);
    d.app.run_action(palette::Action::Scratchpad);
    let count = d.app.workflow.state().unwrap().notes.len();
    d.key(KeyCode::Char('n'));
    assert_eq!(d.app.workflow.state().unwrap().notes.len(), count);
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('e'));
    assert_eq!(d.app.overlay, Overlay::Scratchpad);
    d.app.run_action(palette::Action::Handoff);
    d.key(KeyCode::Enter);
    assert!(!d.root.join("notes/handoff.md").exists());
    d.key(KeyCode::Char('e'));
    assert_eq!(d.app.overlay, Overlay::None);
    assert_eq!(
        fs::read_to_string(source)
            .unwrap()
            .matches("Owned secondary")
            .count(),
        1
    );
}
#[cfg(unix)]
#[test]
fn writingdeck_workflow_external_alias_collision_refuses_to_guess_and_rescues_both_original_contexts()
 {
    use std::os::unix::fs::symlink;
    let (mut d, research) = secondary_book("workflow-aliases", 120, 40, false);
    let (_, note) = workflow_note(&mut d, "Original note source. ");
    d.app.paste("Unsaved note context. ");
    d.app.run_action(palette::Action::Open(research.clone()));
    d.app.paste("Unsaved research context. ");
    let raw = fs::read(&note).unwrap();
    fs::remove_file(&research).unwrap();
    symlink(&note, &research).unwrap();
    d.app.run_action(palette::Action::Scratchpad);
    assert!(
        d.app
            .scratch_body(d.app.workflow.selected.unwrap())
            .is_err()
    );
    d.key(KeyCode::Enter);
    assert_eq!(d.app.overlay, Overlay::Scratchpad);
    assert_eq!(fs::read(&note).unwrap(), raw);
    for (path, text) in [(&note, "Unsaved note"), (&research, "Unsaved research")] {
        let copy = recovery::dir(&d.root).join(path.strip_prefix(&d.root).unwrap());
        assert!(fs::read_to_string(copy).unwrap().contains(text));
    }
}
#[test]
fn writingdeck_workflow_non_utf8_original_cannot_be_promoted_or_replaced() {
    let (mut d, _) = secondary_book("workflow-utf8", 100, 30, false);
    let (id, path) = workflow_note(&mut d, "Before external bytes. ");
    workflow_link(&mut d, id);
    d.app.hide_secondary();
    fs::write(&path, b"\xff\xfe external source").unwrap();
    d.app.check_documents_now();
    d.app.check_documents_now();
    d.app.run_action(palette::Action::Scratchpad);
    assert!(d.app.scratch_body(id).is_err());
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('p'));
    assert_eq!(fs::read(path).unwrap(), b"\xff\xfe external source");
    assert!(
        !d.root
            .join(grimoire_core::workflow::promotion_path(id))
            .exists()
    );
}
#[test]
fn writingdeck_workflow_handoff_four_size_actual_original_reopen_and_context_resume() {
    use grimoire_core::documents::Pane;
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut d, _) = secondary_book(&format!("handoff-sizes-{width}"), width, height, true);
        d.app.editor.place(13, 4);
        d.app.editor.scroll = 7;
        let manuscript = fs::read(first_scene(&d.root)).unwrap();
        workflow_intention(&mut d, "Tomorrow, follow the original river scene. ");
        let path = d.root.join("notes/handoff.md");
        let original = fs::read(&path).unwrap();
        assert!(!journal_path(&d).exists());
        assert!(d.app.try_quit());
        let root = d.root.clone();
        let mut reopened = Desk::open(root, width, height);
        assert_eq!(reopened.app.overlay, Overlay::Handoff);
        assert!(reopened.shows("Tomorrow, follow the original river scene"));
        reopened.key(KeyCode::Enter);
        reopened.app.paste("Real loaded handoff edit. ");
        reopened.ctrl('s');
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("Real loaded handoff")
        );
        reopened.ctrl('z');
        reopened.ctrl('s');
        assert_eq!(fs::read(&path).unwrap(), original);
        reopened.app.run_action(palette::Action::Handoff);
        reopened.key(KeyCode::Char('g'));
        assert_eq!(reopened.app.overlay, Overlay::None);
        assert_eq!(reopened.app.views.active, Pane::Primary);
        assert_eq!(reopened.app.editor.cy, 13);
        assert_eq!(reopened.app.editor.cx, 4);
        assert_eq!(fs::read(first_scene(&reopened.root)).unwrap(), manuscript);
    }
}
#[test]
fn writingdeck_workflow_handoff_requires_nonempty_prose_and_supports_no_manuscript_context() {
    let (mut d, research) = secondary_book("handoff-no-context", 100, 30, false);
    d.app.run_action(palette::Action::Handoff);
    d.key(KeyCode::Char('s'));
    assert!(d.app.workflow.state().unwrap().handoff.intention.is_none());
    d.app.run_action(palette::Action::Open(research));
    workflow_intention(&mut d, "Work on a note next time. ");
    assert!(
        d.app
            .workflow
            .state()
            .unwrap()
            .handoff
            .intention
            .as_ref()
            .unwrap()
            .position
            .is_none()
    );
    let original = fs::read(d.root.join("notes/handoff.md")).unwrap();
    d.key(KeyCode::Char('o'));
    assert!(!d.app.workflow.state().unwrap().handoff.show_on_open);
    let root = d.root.clone();
    let reopened = Desk::open(root, 100, 30);
    assert_ne!(reopened.app.overlay, Overlay::Handoff);
    assert_eq!(
        fs::read(reopened.root.join("notes/handoff.md")).unwrap(),
        original
    );
}
#[test]
fn writingdeck_workflow_handoff_does_not_override_source_recovery_priority() {
    let (mut d, _) = secondary_book("handoff-recovery", 100, 30, false);
    workflow_intention(&mut d, "Recover first, intention later. ");
    let primary = first_scene(&d.root);
    let saved = fs::read_to_string(&primary).unwrap();
    recovery::keep(
        &d.root,
        &primary,
        &format!("{saved}\nRecovered unsaved original.\n"),
    )
    .unwrap();
    let root = d.root.clone();
    let reopened = Desk::open(root, 100, 30);
    assert!(matches!(reopened.app.overlay, Overlay::Recover { .. }));
}
#[test]
fn writingdeck_workflow_handoff_missing_source_is_reported_not_recreated_on_startup() {
    let (mut d, _) = secondary_book("handoff-missing", 100, 30, false);
    workflow_intention(&mut d, "This original may move externally. ");
    let path = d.root.join("notes/handoff.md");
    d.app.hide_secondary();
    fs::remove_file(&path).unwrap();
    let root = d.root.clone();
    let reopened = Desk::open(root, 100, 30);
    assert_eq!(reopened.app.overlay, Overlay::Handoff);
    assert!(!path.exists());
    assert!(reopened.app.handoff_body().is_err());
}
#[test]
fn writingdeck_workflow_book_switch_separates_all_original_notes_and_handoff_metadata() {
    let (mut d, _) = secondary_book("workflow-first", 100, 30, false);
    workflow_note(&mut d, "Private first-book scratch. ");
    workflow_intention(&mut d, "Private first-book intention. ");
    let other = Desk::open(book("workflow-second", true), 100, 30);
    assert!(d.app.open_book(&other.root));
    assert!(d.app.workflow.state().unwrap().notes.is_empty());
    assert!(d.app.workflow.state().unwrap().handoff.intention.is_none());
    d.app.run_action(palette::Action::Handoff);
    assert!(
        !d.app
            .handoff_body()
            .is_ok_and(|b| b.contains("Private first-book"))
    );
}
#[test]
fn writingdeck_workflow_lessons_require_real_promotion_and_handoff_reopen_in_safe_demo() {
    use crate::app::writingdeck::Page;
    let root = std::env::temp_dir().join(format!("wd-workflow-tutorial-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    grimoire_core::tutorial::create_demo(&root).unwrap();
    let mut d = Desk::open(root, 120, 40);
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 15 });
    d.key(KeyCode::Enter);
    assert!(!d.app.deck.progress.completed.contains(&15));
    d.key(KeyCode::Char('n'));
    d.app.paste("Actual safe-demo idea. ");
    d.ctrl('s');
    d.app.run_action(palette::Action::Scratchpad);
    d.key(KeyCode::Char('p'));
    assert!(!d.app.deck.progress.completed.contains(&15));
    let id = d.app.workflow.selected.unwrap();
    let scene = d
        .app
        .project
        .nodes
        .iter()
        .find(|n| n.in_manuscript && n.kind == project::Kind::Scene)
        .unwrap()
        .path
        .clone();
    d.app.link_scratchpad(id, &scene);
    d.key(KeyCode::Char('p'));
    assert!(d.app.deck.progress.completed.contains(&15));
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 16 });
    d.key(KeyCode::Enter);
    assert!(!d.app.deck.progress.completed.contains(&16));
    workflow_intention(&mut d, "An original next-open intention in the demo. ");
    assert!(!d.app.deck.progress.completed.contains(&16));
    assert!(d.app.try_quit());
    let root = d.root.clone();
    let reopened = Desk::open(root, 120, 40);
    assert_eq!(reopened.app.overlay, Overlay::Handoff);
    assert!(reopened.app.deck.progress.completed.contains(&16));
    assert!(
        grimoire_core::tutorial::Store::load(&reopened.root)
            .unwrap()
            .progress
            .completed
            .contains(&16)
    );
}
#[test]
fn writingdeck_workflow_lessons_never_launch_writing_exercises_on_an_ordinary_novel() {
    use crate::app::writingdeck::Page;
    let (mut d, _) = secondary_book("workflow-real", 100, 30, false);
    let before = fs::read(first_scene(&d.root)).unwrap();
    for id in [15, 16] {
        d.app.overlay = Overlay::Deck(Page::Lesson { id });
        d.key(KeyCode::Enter);
        assert!(d.app.deck.note.contains("safe demo"));
        assert!(d.app.deck.progress.active.is_none());
    }
    assert_eq!(fs::read(first_scene(&d.root)).unwrap(), before);
    assert!(!d.root.join("notes/scratchpad").exists());
    assert!(!d.root.join("notes/handoff.md").exists());
}
#[test]
fn writingdeck_workflow_starters_populate_real_original_panes_without_replacing_existing_prose() {
    use grimoire_core::{documents::Pane, workspaces::Preset};
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let (mut d, _) = secondary_book(&format!("workflow-starters-{width}"), width, height, true);
        let primary = fs::read(first_scene(&d.root)).unwrap();
        for (slot, preset) in Preset::ALL.into_iter().enumerate() {
            d.app.apply_starter_preset(slot, preset);
            assert_eq!(d.app.overlay, Overlay::None, "{}", d.app.deck.note);
            assert_eq!(d.app.deck.state.selected, slot);
            if let Some((path, _)) = preset.source() {
                assert_eq!(
                    d.app.project.nodes[d.app.view_open(Pane::Secondary).unwrap()].path,
                    d.root.join(path)
                );
                d.app.activate_editor(Pane::Secondary);
                d.app.paste("Keep this original preset note. ");
                d.ctrl('s');
                let saved = fs::read(d.root.join(path)).unwrap();
                d.app.apply_starter_preset(slot, preset);
                assert_eq!(fs::read(d.root.join(path)).unwrap(), saved);
            } else {
                assert!(d.app.focus_mode);
                assert!(!d.app.views.secondary_visible);
                assert!(d.app.view_open(Pane::Secondary).is_some());
            }
            assert_eq!(fs::read(first_scene(&d.root)).unwrap(), primary);
        }
    }
}
#[test]
fn writingdeck_workflow_starter_refuses_unknown_or_concurrently_changed_layout_before_creating_sources()
 {
    use grimoire_core::workspaces::{Preset, Store};
    let (mut d, _) = secondary_book("workflow-preset-conflict", 100, 30, false);
    let path = Store::path(&d.root);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "Future external layout").unwrap();
    d.app.apply_starter_preset(1, Preset::Planning);
    assert!(!d.root.join("notes/workspaces/planning.md").exists());
    assert_eq!(fs::read_to_string(&path).unwrap(), "Future external layout");
    let mut reopened = Desk::open(d.root.clone(), 100, 30);
    reopened.app.apply_starter_preset(2, Preset::Research);
    assert!(!reopened.root.join("research/workspace.md").exists());
    assert_eq!(fs::read_to_string(path).unwrap(), "Future external layout");
}
#[test]
fn writingdeck_workflow_workspace_pin_sets_filter_without_changing_global_card_lifecycle() {
    use grimoire_core::pins::Scope;
    let (mut d, research) = secondary_book("workflow-pinsets", 100, 30, false);
    d.app.pin_document(&first_scene(&d.root), None);
    let first = d.app.pins.state().pins[0].id;
    d.app.pin_document(&research, None);
    let second = d.app.pins.state().pins[1].id;
    assert_eq!(d.app.pin_count(false), 2);
    d.app.save_workspace_pin_set(0, Some(vec![first])).unwrap();
    d.app.save_workspace_pin_set(1, Some(vec![second])).unwrap();
    assert_eq!(d.app.pin_ids(false), vec![first]);
    d.app.deck.state.selected = 1;
    assert_eq!(d.app.pin_ids(false), vec![second]);
    assert_eq!(d.app.pins.state().pins.len(), 2);
    assert!(
        d.app
            .pins
            .state()
            .pins
            .iter()
            .all(|p| p.scope == Scope::Novel && p.life == grimoire_core::pins::Life::Active)
    );
    d.app.save_workspace_pin_set(1, None).unwrap();
    assert_eq!(d.app.pin_count(false), 2);
    let persisted = grimoire_core::workspaces::Store::load(&d.root).unwrap();
    assert_eq!(persisted.state.workspaces[0].pin_ids, Some(vec![first]));
    assert!(persisted.state.workspaces[1].pin_ids.is_none());
    d.app.save_workspace_pin_set(1, Some(vec![second])).unwrap();
    d.app.overlay = Overlay::None;
    d.app.focus = Focus::Pins;
    d.key(KeyCode::Enter);
    assert_eq!(d.app.project.nodes[d.app.open.unwrap()].path, research);
}
#[test]
fn writingdeck_workflow_workspace_pin_picker_cancel_and_save_are_explicit() {
    use crate::app::writingdeck::Page;
    let (mut d, _) = secondary_book("workflow-pin-picker", 80, 24, false);
    d.app.pin_document(&first_scene(&d.root), None);
    d.app.overlay = Overlay::Deck(Page::Workspaces { sel: 0 });
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('z'));
    d.key(KeyCode::Esc);
    assert!(d.app.deck.state.current().pin_ids.is_none());
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('z'));
    d.key(KeyCode::Enter);
    assert_eq!(d.app.deck.state.current().pin_ids, Some(vec![]));
    assert_eq!(d.app.pin_count(false), 0);
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('A'));
    assert!(d.app.deck.state.current().pin_ids.is_none());
    assert_eq!(d.app.pin_count(false), 1);
}

#[test]
fn writingdeck_workflow_command_palette_remains_reachable_from_read_only_readers() {
    let (mut d, _) = secondary_book("workflow-modal-palette", 80, 24, false);
    let before = fs::read(first_scene(&d.root)).unwrap();
    for overlay in [
        Overlay::Pins,
        Overlay::Journal,
        Overlay::Scratchpad,
        Overlay::Handoff,
    ] {
        d.app.overlay = overlay;
        d.ctrl('k');
        assert!(matches!(d.app.overlay, Overlay::Palette { .. }));
        d.key(KeyCode::Esc);
    }
    assert_eq!(fs::read(first_scene(&d.root)).unwrap(), before);
    assert!(d.app.workflow.state().unwrap().notes.is_empty());
}

// ---- real M5 operation ledger, goals, calendars, sprints and challenges ----
#[derive(Clone)]
struct MetricClock(std::sync::Arc<std::sync::Mutex<grimoire_core::accounting::Moment>>);
impl grimoire_core::accounting::Clock for MetricClock {
    fn now(&self) -> grimoire_core::accounting::Moment {
        self.0.lock().unwrap().clone()
    }
}
fn metric_clock(d: &mut Desk) -> MetricClock {
    let clock = MetricClock(std::sync::Arc::new(std::sync::Mutex::new(
        grimoire_core::accounting::Moment {
            mono_ms: 0,
            wall: chrono::DateTime::parse_from_rfc3339("2026-10-25T12:00:00+00:00").unwrap(),
            zone: Some("UTC".into()),
        },
    )));
    d.app.productivity.clock = Box::new(clock.clone());
    d.app.refresh_productivity_snapshot();
    clock
}
fn metric_advance(clock: &MetricClock, ms: u64) {
    let mut t = clock.0.lock().unwrap();
    t.mono_ms += ms;
    t.wall += chrono::Duration::milliseconds(ms as i64);
}
fn metric_totals(
    d: &Desk,
    role: grimoire_core::accounting::Role,
) -> grimoire_core::accounting::Totals {
    grimoire_core::accounting::Totals::collect(
        d.app.productivity.ledger.known.iter(),
        "0001-01-01",
        "9999-12-31",
        role,
    )
}
fn metric_writer(tag: &str, w: u16, h: u16) -> Desk {
    let mut d = Desk::open(book(tag, true), w, h);
    d.app.overlay = Overlay::None;
    d.app.focus = d.app.editor_focus();
    d.app.editor.scene_end();
    d
}
fn metric_value(d: &Desk, metric: grimoire_core::productivity::Metric) -> Option<i128> {
    d.app.productivity.snapshot.lifetime(metric, None, false)
}
fn metric_field(d: &mut Desk, text: &str) {
    let old = match &d.app.overlay {
        Overlay::ProductivityInput { buf, .. } => buf.len(),
        _ => panic!("expected target/date input"),
    };
    for _ in 0..old {
        d.key(KeyCode::Backspace);
    }
    d.typed(text);
    d.key(KeyCode::Enter);
}
fn metric_new_sprint(d: &mut Desk, index: usize, target: &str) {
    d.app.run_action(palette::Action::AdvancedSprints);
    d.key(KeyCode::Char('n'));
    for _ in 0..index {
        d.key(KeyCode::Down);
    }
    d.key(KeyCode::Enter);
    metric_field(d, target);
    assert!(d.app.productivity.sprint.is_some(), "{}", d.app.msg);
}

#[test]
fn writingdeck_productivity_keyboard_paste_undo_are_actual_source_operations_not_net_baselines() {
    use grimoire_core::accounting::Role;
    let mut d = metric_writer("metrics-origins", 100, 30);
    metric_clock(&mut d);
    d.typed("alpha beta ");
    d.app.paste("gamma delta ");
    d.ctrl('z');
    d.ctrl('s');
    let t = metric_totals(&d, Role::Manuscript);
    assert_eq!(
        (
            t.typed.added,
            t.pasted.added,
            t.replay.removed,
            t.net(false)
        ),
        (2, 2, 2, 2)
    );
    assert_eq!(d.app.productivity.ledger.pending_count(), 0);
    for part in fs::read_dir(d.root.join(".grimoire/writingdeck/ledger/2026-10-25")).unwrap() {
        let text = fs::read_to_string(part.unwrap().path()).unwrap();
        assert!(
            !text.contains("alpha")
                && !text.contains("beta")
                && !text.contains("gamma")
                && !text.contains("delta"),
            "metadata must not copy source prose"
        );
    }
    d.app.run_action(palette::Action::Productivity);
    d.draw();
    assert!(d.shows("Productivity"));
    assert!(
        d.shows("Keyboard-pair")
            || d.app
                .productivity
                .rows
                .iter()
                .any(|r| r.contains("NOT a claim of proven exclusive"))
    );
}
#[test]
fn writingdeck_productivity_both_editor_sources_are_attributed_and_hidden_notes_never_become_manuscript_words()
 {
    use grimoire_core::accounting::Role;
    let (mut d, note) = secondary_book("metrics-two-panes", 120, 40, false);
    metric_clock(&mut d);
    d.app.focus = d.app.editor_focus();
    d.app.editor.scene_end();
    d.typed(" primaryAdded ");
    choose_secondary(&mut d, &note);
    d.app.editor.scene_end();
    d.typed(" sideAdded ");
    d.app.hide_secondary();
    d.ctrl('s');
    assert_eq!(metric_totals(&d, Role::Manuscript).typed.added, 1);
    assert_eq!(metric_totals(&d, Role::Research).typed.added, 1);
    assert!(fs::read_to_string(note).unwrap().contains("sideAdded"));
    let entries = &d.app.productivity.ledger.known;
    assert!(
        entries
            .iter()
            .any(|e| e.source.path.starts_with("research"))
    );
    assert!(
        entries
            .iter()
            .any(|e| e.source.path.starts_with("manuscript"))
    );
}
#[test]
fn writingdeck_productivity_formatting_find_and_book_undo_do_not_reauthor_untouched_words() {
    use grimoire_core::accounting::Role;
    let mut d = metric_writer("metrics-replace", 120, 40);
    metric_clock(&mut d);
    d.app.paste("cat unchanged other words cat");
    d.ctrl('s');
    d.app.editor.place(0, 1);
    d.ctrl('b');
    d.ctrl('s');
    let t = metric_totals(&d, Role::Manuscript);
    assert_eq!(t.input.added, 5, "format markers cannot add existing words");
    d.app.overlay = Overlay::None;
    d.app.open_find_book("cat".into());
    if let Overlay::FindBook { with, .. } = &mut d.app.overlay {
        *with = Some("dog".into());
    }
    d.app.replace_all_key();
    d.app.on_overlay_key(app::Key::Char('y'));
    d.app.save();
    let t = metric_totals(&d, Role::Manuscript);
    assert_eq!((t.input.added, t.input.removed), (7, 2));
    d.app.overlay = Overlay::None;
    d.ctrl('z');
    d.ctrl('s');
    let t = metric_totals(&d, Role::Manuscript);
    assert_eq!(t.replay.net(false), 0);
    assert_eq!((t.replay.added, t.replay.removed), (2, 2));
    assert_eq!(t.input.gaps + t.replay.gaps, 0);
}
#[test]
fn writingdeck_productivity_idle_cutoff_pause_and_revision_mode_use_injected_monotonic_events() {
    use grimoire_core::accounting::Role;
    let mut d = metric_writer("metrics-clock", 100, 30);
    let clock = metric_clock(&mut d);
    d.typed("a");
    metric_advance(&clock, 500);
    d.typed("b");
    metric_advance(&clock, 61000);
    d.typed("c");
    d.ctrl('s');
    assert_eq!(metric_totals(&d, Role::Manuscript).active_ms, 500);
    d.app.run_action(palette::Action::Productivity);
    d.key(KeyCode::Char('m'));
    assert_eq!(
        d.app.productivity.config.mode,
        grimoire_core::accounting::Mode::Revision
    );
    d.key(KeyCode::Esc);
    metric_advance(&clock, 1000);
    d.typed("d");
    d.ctrl('s');
    assert_eq!(metric_totals(&d, Role::Manuscript).revision_operations, 1);
    assert_eq!(metric_totals(&d, Role::Manuscript).active_ms, 500);
}
#[test]
fn writingdeck_productivity_calendar_goal_and_all_six_pages_are_real_at_four_sizes() {
    use crate::app::productivity::Page;
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = metric_writer(&format!("metrics-pages-{w}"), w, h);
        metric_clock(&mut d);
        let before = d.app.editor.text();
        d.ctrl('k');
        d.typed("calendar and rest days");
        d.key(KeyCode::Enter);
        assert!(matches!(
            d.app.overlay,
            Overlay::Productivity {
                page: Page::Calendar,
                ..
            }
        ));
        d.key(KeyCode::Char('n'));
        assert_eq!(
            d.app.productivity.config.schedule.as_ref().unwrap().target,
            50000
        );
        d.key(KeyCode::Char(' '));
        assert!(
            d.app
                .productivity
                .config
                .schedule
                .as_ref()
                .unwrap()
                .rest_days
                .contains("2026-11-01")
        );
        assert!(d.shows("Calendar"));
        d.key(KeyCode::Char('2'));
        d.key(KeyCode::Char('n'));
        d.key(KeyCode::Enter);
        metric_field(&mut d, "25");
        assert_eq!(d.app.productivity.config.goals[0].target, 25);
        for key in ['1', '2', '3', '4', '5', '6'] {
            d.key(KeyCode::Char(key));
            assert!(matches!(d.app.overlay, Overlay::Productivity { .. }));
            assert!(!d.rows().iter().all(|r| r.trim().is_empty()));
        }
        assert_eq!(d.app.editor.text(), before);
        assert!(d.app.try_quit());
        let reopened = Desk::open(d.root.clone(), w, h);
        assert_eq!(reopened.app.productivity.config.goals[0].target, 25);
        assert!(
            reopened
                .app
                .productivity
                .config
                .schedule
                .as_ref()
                .unwrap()
                .rest_days
                .contains("2026-11-01")
        );
    }
}
#[test]
fn writingdeck_productivity_calendar_replacement_keeps_prior_rest_day_configuration() {
    let mut d = metric_writer("metrics-cal-history", 100, 30);
    metric_clock(&mut d);
    d.app.run_action(palette::Action::Calendar);
    d.key(KeyCode::Char('n'));
    d.key(KeyCode::Char(' '));
    d.key(KeyCode::Char('n'));
    assert_eq!(d.app.productivity.config.schedule_history.len(), 1);
    assert!(
        d.app.productivity.config.schedule_history[0]
            .rest_days
            .contains("2026-11-01")
    );
    assert!(
        d.app
            .productivity
            .config
            .schedule
            .as_ref()
            .unwrap()
            .rest_days
            .is_empty()
    );
}
#[test]
fn writingdeck_productivity_invalid_target_or_date_keeps_original_settings_and_field() {
    let mut d = metric_writer("metrics-bad-input", 100, 30);
    metric_clock(&mut d);
    d.app.run_action(palette::Action::Calendar);
    d.key(KeyCode::Char('n'));
    let old = fs::read(d.root.join(".grimoire/writingdeck/productivity.json")).unwrap();
    d.key(KeyCode::Char('e'));
    metric_field(&mut d, "2026-02-30");
    assert!(matches!(d.app.overlay, Overlay::ProductivityInput { .. }));
    assert_eq!(
        fs::read(d.root.join(".grimoire/writingdeck/productivity.json")).unwrap(),
        old
    );
    d.key(KeyCode::Esc);
    d.key(KeyCode::Char('2'));
    d.key(KeyCode::Char('n'));
    d.key(KeyCode::Enter);
    metric_field(&mut d, "0");
    assert!(d.app.productivity.config.goals.is_empty());
    assert!(matches!(d.app.overlay, Overlay::ProductivityInput { .. }));
}
#[test]
fn writingdeck_productivity_measured_daily_completion_needs_accepted_subsequent_manuscript_work() {
    use grimoire_core::challenges::{Evidence, Status};
    let mut d = metric_writer("metrics-challenge", 120, 40);
    metric_clock(&mut d);
    d.app.run_action(palette::Action::DailyChallenges);
    assert_eq!(
        d.app.productivity.day.as_ref().unwrap().value.assignments[0].record_id,
        "productivity-01"
    );
    d.key(KeyCode::Char('c'));
    assert_eq!(
        d.app.productivity.day.as_ref().unwrap().value.assignments[0].status,
        Status::Assigned
    );
    d.key(KeyCode::Char('a'));
    d.key(KeyCode::Char('c'));
    assert_eq!(
        d.app.productivity.day.as_ref().unwrap().value.assignments[0].status,
        Status::Accepted
    );
    d.key(KeyCode::Esc);
    d.app.paste(&"word ".repeat(50));
    d.ctrl('s');
    d.app.run_action(palette::Action::DailyChallenges);
    d.key(KeyCode::Char('c'));
    let a = &d.app.productivity.day.as_ref().unwrap().value.assignments[0];
    assert_eq!(a.status, Status::Completed, "{}", d.app.msg);
    assert_eq!(a.evidence, Some(Evidence::Measured));
    let awards = d.app.productivity.config.awards.len();
    d.key(KeyCode::Char('c'));
    assert_eq!(d.app.productivity.config.awards.len(), awards);
    let before = fs::read(
        d.root
            .join(".grimoire/writingdeck/challenges/2026-10-25.json"),
    )
    .unwrap();
    let mut restarted = Desk::open(d.root.clone(), 120, 40);
    metric_clock(&mut restarted);
    restarted.app.run_action(palette::Action::DailyChallenges);
    assert_eq!(
        fs::read(
            d.root
                .join(".grimoire/writingdeck/challenges/2026-10-25.json")
        )
        .unwrap(),
        before
    );
    assert_eq!(
        restarted
            .app
            .productivity
            .day
            .as_ref()
            .unwrap()
            .value
            .assignments[0]
            .evidence,
        Some(Evidence::Measured)
    );
}
#[test]
fn writingdeck_productivity_creative_completion_stays_explicitly_self_reported() {
    use grimoire_core::challenges::{Category, Evidence, Status};
    let mut d = metric_writer("metrics-selfreport", 100, 30);
    metric_clock(&mut d);
    d.app
        .productivity
        .update(|s| {
            s.challenge_profile.categories = [Category::Creative].into_iter().collect();
            s.challenge_profile.difficulty = 1;
            Ok(())
        })
        .unwrap();
    d.app.run_action(palette::Action::DailyChallenges);
    d.key(KeyCode::Char('a'));
    d.key(KeyCode::Char('c'));
    assert_eq!(
        d.app.productivity.day.as_ref().unwrap().value.assignments[0].status,
        Status::Accepted
    );
    d.key(KeyCode::Char('x'));
    assert_eq!(
        d.app.productivity.day.as_ref().unwrap().value.assignments[0].evidence,
        Some(Evidence::SelfReported)
    );
    assert_eq!(
        metric_value(&d, grimoire_core::productivity::Metric::InputAdded),
        Some(0)
    );
}
#[test]
fn writingdeck_productivity_reroll_is_explicit_preserves_acceptance_and_pasted_commands_are_ignored()
 {
    let mut d = metric_writer("metrics-reroll", 100, 30);
    metric_clock(&mut d);
    d.app.run_action(palette::Action::DailyChallenges);
    d.key(KeyCode::Char('a'));
    let old = d.app.productivity.day.as_ref().unwrap().value.assignments[0].clone();
    d.key(KeyCode::Char('R'));
    let day = &d.app.productivity.day.as_ref().unwrap().value;
    assert_ne!(day.assignments[0].record_id, old.record_id);
    assert_eq!(day.rerolls[0].old_assignment, old);
    let source = d.app.editor.text();
    let count = day.rerolls.len();
    let selected = d.app.editor.selection();
    d.ctrl('x');
    d.ctrl('v');
    d.ctrl('z');
    d.ctrl('r');
    d.app.on_drag(50, 10);
    assert_eq!(d.app.editor.text(), source);
    assert_eq!(d.app.editor.selection(), selected);
    assert_eq!(
        d.app.productivity.day.as_ref().unwrap().value.rerolls.len(),
        count
    );
}
#[test]
fn writingdeck_productivity_filter_draft_cancellation_and_explicit_save_do_not_regenerate_today() {
    let mut d = metric_writer("metrics-filters", 100, 30);
    metric_clock(&mut d);
    d.app.run_action(palette::Action::DailyChallenges);
    let old = d.app.productivity.day.as_ref().unwrap().value.clone();
    d.key(KeyCode::Char('f'));
    d.key(KeyCode::Char(' '));
    d.key(KeyCode::Esc);
    assert_eq!(
        d.app.productivity.config.challenge_profile.categories.len(),
        10
    );
    d.key(KeyCode::Char('f'));
    d.key(KeyCode::Char(' '));
    d.key(KeyCode::Enter);
    assert_eq!(
        d.app.productivity.config.challenge_profile.categories.len(),
        9
    );
    assert_eq!(d.app.productivity.day.as_ref().unwrap().value, old);
}
#[test]
fn writingdeck_productivity_word_sprint_pause_resume_and_breaks_preserve_edits_and_real_time() {
    use grimoire_core::productivity::SprintStatus;
    let mut d = metric_writer("metrics-sprint", 100, 30);
    let clock = metric_clock(&mut d);
    metric_new_sprint(&mut d, 1, "3");
    d.key(KeyCode::Esc);
    d.app.paste("one ");
    d.ctrl('s');
    d.app.run_action(palette::Action::AdvancedSprints);
    metric_advance(&clock, 1000);
    d.key(KeyCode::Char('p'));
    assert_eq!(
        d.app.productivity.sprint.as_ref().unwrap().status,
        SprintStatus::Paused
    );
    d.key(KeyCode::Esc);
    d.app.paste("excluded ");
    d.ctrl('s');
    d.app.run_action(palette::Action::AdvancedSprints);
    metric_advance(&clock, 1000);
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Char('b'));
    metric_field(&mut d, "1");
    assert!(
        d.app
            .productivity
            .sprint
            .as_ref()
            .unwrap()
            .break_timer
            .is_some()
    );
    metric_advance(&clock, 60_000);
    assert!(d.app.checkpoint_productivity(true));
    assert_eq!(d.app.productivity.sprint.as_ref().unwrap().breaks.len(), 1);
    assert!(
        d.app
            .productivity
            .sprint
            .as_ref()
            .unwrap()
            .effective_paused()
    );
    d.key(KeyCode::Char('p'));
    d.key(KeyCode::Esc);
    d.app.paste("two three ");
    d.ctrl('s');
    let s = d.app.productivity.sprint.as_ref().unwrap();
    assert_eq!(s.status, SprintStatus::Completed);
    assert_eq!(s.progress, 3);
    assert_eq!(s.elapsed_ms, 1000);
    assert!(d.app.editor.text().contains("excluded"));
}
#[test]
fn writingdeck_productivity_no_editing_experiment_reports_deletions_without_blocking_or_losing_them()
 {
    let mut d = metric_writer("metrics-noedit", 100, 30);
    let clock = metric_clock(&mut d);
    d.app.paste("existing words");
    d.ctrl('s');
    metric_new_sprint(&mut d, 3, "1");
    d.key(KeyCode::Esc);
    d.app.editor.scene_end();
    d.key(KeyCode::Backspace);
    d.ctrl('s');
    assert!(
        d.app
            .productivity
            .sprint
            .as_ref()
            .unwrap()
            .deletion_observed
    );
    assert!(!d.app.editor.text().ends_with("words"));
    metric_advance(&clock, 60_000);
    d.app.checkpoint_productivity(true);
    let s = d.app.productivity.sprint.as_ref().unwrap();
    assert_eq!(
        s.status,
        grimoire_core::productivity::SprintStatus::Completed
    );
    assert!(
        !d.app
            .productivity
            .config
            .awards
            .iter()
            .any(|a| a.id == format!("sprint-{}", s.id))
    );
    d.ctrl('z');
    assert!(d.app.editor.text().ends_with("words"));
}
#[test]
fn writingdeck_productivity_sprint_restart_requires_resume_and_never_adds_offline_time() {
    let mut d = metric_writer("metrics-sprint-restart", 100, 30);
    let clock = metric_clock(&mut d);
    metric_new_sprint(&mut d, 0, "1");
    metric_advance(&clock, 5000);
    d.app.checkpoint_productivity(true);
    assert!(d.app.try_quit());
    let mut restarted = Desk::open(d.root.clone(), 100, 30);
    let clock = metric_clock(&mut restarted);
    metric_advance(&clock, 3_600_000);
    restarted.app.checkpoint_productivity(true);
    let s = restarted.app.productivity.sprint.as_ref().unwrap();
    assert!(s.effective_paused());
    assert_eq!(s.elapsed_ms, 5000);
    restarted.app.run_action(palette::Action::AdvancedSprints);
    restarted.key(KeyCode::Char('p'));
    metric_advance(&clock, 55000);
    restarted.app.checkpoint_productivity(true);
    assert_eq!(
        restarted
            .app
            .productivity
            .sprint
            .as_ref()
            .unwrap()
            .elapsed_ms,
        60000
    );
}
#[test]
fn writingdeck_productivity_failed_ledger_write_keeps_pending_records_blocks_normal_quit_and_retry_is_idempotent()
 {
    let mut d = metric_writer("metrics-ledger-fail", 100, 30);
    metric_clock(&mut d);
    d.typed("first ");
    d.ctrl('s');
    let directory = d.root.join(".grimoire/writingdeck/ledger/2026-10-25");
    let file = fs::read_dir(&directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let old = fs::read(&file).unwrap();
    let future = b"{\"version\":999,\"retain\":true}";
    fs::write(&file, future).unwrap();
    d.typed("second ");
    d.ctrl('s');
    assert!(
        fs::read_to_string(first_scene(&d.root))
            .unwrap()
            .contains("second")
    );
    assert!(d.app.productivity.ledger.pending_count() > 0);
    assert!(!d.app.try_quit());
    assert_eq!(fs::read(&file).unwrap(), future);
    fs::write(&file, old).unwrap();
    d.app.reload_productivity();
    assert_eq!(d.app.productivity.ledger.pending_count(), 0);
    assert_eq!(
        metric_totals(&d, grimoire_core::accounting::Role::Manuscript)
            .typed
            .added,
        2
    );
    assert!(d.app.try_quit());
    assert_eq!(
        grimoire_core::accounting::Ledger::load(&d.root).known.len(),
        2
    );
}
#[test]
fn writingdeck_productivity_unknown_settings_are_preserved_and_never_stop_original_markdown_saving()
{
    let root = book("metrics-future-settings", true);
    fs::create_dir_all(root.join(".grimoire/writingdeck")).unwrap();
    let future = b"{\"version\":999,\"future\":{\"retain\":true}}";
    fs::write(root.join(".grimoire/writingdeck/productivity.json"), future).unwrap();
    let mut d = Desk::open(root, 100, 30);
    d.app.focus = d.app.editor_focus();
    d.typed("original safe words");
    d.ctrl('s');
    assert!(d.app.productivity.store.is_none());
    assert!(d.app.try_quit());
    assert_eq!(
        fs::read(d.root.join(".grimoire/writingdeck/productivity.json")).unwrap(),
        future
    );
    assert!(
        fs::read_to_string(first_scene(&d.root))
            .unwrap()
            .contains("original safe words")
    );
    assert!(d.app.productivity.ledger.known.is_empty());
}
#[test]
fn writingdeck_productivity_optional_badge_write_failure_does_not_punish_or_block_saved_prose_and_ledger()
 {
    let mut d = metric_writer("metrics-optional-fail", 100, 30);
    metric_clock(&mut d);
    let file = d.root.join(".grimoire/writingdeck/productivity.json");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let future = b"{\"version\":999,\"keep\":true}";
    fs::write(&file, future).unwrap();
    d.typed("genuine word");
    d.ctrl('s');
    assert_eq!(d.app.productivity.ledger.pending_count(), 0);
    assert!(d.app.try_quit(), "{}", d.app.msg);
    assert_eq!(fs::read(file).unwrap(), future);
    assert!(
        fs::read_to_string(first_scene(&d.root))
            .unwrap()
            .contains("genuine word")
    );
}
#[test]
fn writingdeck_productivity_disable_everything_keeps_source_typing_and_existing_collection() {
    let mut d = metric_writer("metrics-disabled", 100, 30);
    metric_clock(&mut d);
    d.typed("first ");
    d.ctrl('s');
    let earned = d.app.productivity.config.awards.len();
    assert!(earned > 0);
    let known = d.app.productivity.ledger.known.len();
    d.app.run_action(palette::Action::Productivity);
    for key in ['t', 'a', 'd'] {
        d.key(KeyCode::Char(key));
    }
    assert!(
        !d.app.productivity.config.tracking
            && !d.app.productivity.config.achievements
            && !d.app.productivity.config.challenges
    );
    d.key(KeyCode::Esc);
    d.typed("more source words ");
    d.ctrl('s');
    assert_eq!(d.app.productivity.ledger.known.len(), known);
    assert_eq!(d.app.productivity.config.awards.len(), earned);
    d.app.run_action(palette::Action::DailyChallenges);
    assert!(d.app.productivity.day.is_none());
    assert!(
        !d.root
            .join(".grimoire/writingdeck/challenges/2026-10-25.json")
            .exists()
    );
}
#[test]
fn writingdeck_productivity_draw_is_cached_and_does_not_create_events_or_assignments() {
    let mut d = metric_writer("metrics-pure-draw", 80, 24);
    metric_clock(&mut d);
    let n = d.app.productivity.ledger.known.len();
    for _ in 0..12 {
        d.draw();
    }
    assert_eq!(d.app.productivity.ledger.known.len(), n);
    assert_eq!(d.app.productivity.ledger.pending_count(), 0);
    assert!(!d.root.join(".grimoire/writingdeck/challenges").exists());
    assert!(!d.root.join(".grimoire/writingdeck/ledger").exists());
}
#[test]
fn writingdeck_productivity_current_goals_never_derive_gross_or_time_from_existing_source() {
    let root = book("metrics-native-only", true);
    let scene = first_scene(&root);
    fs::write(
        &scene,
        "---\ntitle: Existing\n---\n\nExisting source has five words.\n",
    )
    .unwrap();
    let mut d = Desk::open(root, 100, 30);
    metric_clock(&mut d);
    d.app.run_action(palette::Action::Productivity);
    assert_eq!(
        metric_value(&d, grimoire_core::productivity::Metric::TypedAdded),
        Some(0)
    );
    assert_eq!(
        metric_value(&d, grimoire_core::productivity::Metric::ActiveMinutes),
        Some(0)
    );
    assert_eq!(
        metric_value(&d, grimoire_core::productivity::Metric::ManuscriptWords),
        Some(5)
    );
    assert!(d.app.productivity.ledger.known.is_empty());
}
#[test]
fn writingdeck_productivity_tutorial_18_20_21_require_real_persisted_actions_and_never_a_next_button()
 {
    use crate::app::writingdeck::Page;
    let root = std::env::temp_dir().join(format!("wd-metrics-tutorial-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    grimoire_core::tutorial::create_demo(&root).unwrap();
    let mut d = Desk::open(root, 120, 40);
    metric_clock(&mut d);
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 18 });
    d.key(KeyCode::Enter);
    assert_eq!(d.app.deck.progress.active, Some(18));
    d.key(KeyCode::Char('n'));
    assert!(!d.app.deck.progress.completed.contains(&18));
    d.key(KeyCode::Char(' '));
    assert!(d.app.deck.progress.completed.contains(&18));
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 20 });
    d.key(KeyCode::Enter);
    if !d.app.deck.progress.completed.contains(&20) {
        d.key(KeyCode::Esc);
        d.app.focus = d.app.editor_focus();
        d.app.editor.scene_end();
        d.typed(" lessonWord ");
        d.ctrl('s');
        d.app.run_action(palette::Action::Achievements);
    }
    assert!(d.app.deck.progress.completed.contains(&20));
    assert!(!d.app.productivity.config.awards.is_empty());
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 21 });
    d.key(KeyCode::Enter);
    assert!(!d.app.deck.progress.completed.contains(&21));
    d.key(KeyCode::Char('a'));
    d.key(KeyCode::Esc);
    d.app.focus = d.app.editor_focus();
    d.app.editor.scene_end();
    d.app.paste(&format!(" {}", "exercise ".repeat(50)));
    d.ctrl('s');
    d.app.run_action(palette::Action::DailyChallenges);
    d.key(KeyCode::Char('c'));
    assert!(d.app.deck.progress.completed.contains(&21), "{}", d.app.msg);
    let saved = grimoire_core::tutorial::Store::load(&d.root).unwrap();
    assert!(
        [18, 20, 21]
            .iter()
            .all(|id| saved.progress.completed.contains(id))
    );
}
#[test]
fn writingdeck_productivity_discoverable_short_menu_has_every_tool_and_correct_nested_back() {
    for (w, h) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let mut d = metric_writer(&format!("metrics-menu-{w}"), w, h);
        d.key(KeyCode::Esc);
        d.choose("Writing tools");
        d.choose("Productivity tools");
        assert!(matches!(
            d.app.overlay,
            Overlay::Sub {
                sub: Sub::Productivity,
                ..
            }
        ));
        for i in 0..6 {
            let rows = d.app.menu_rows();
            let label = rows[i].0.clone();
            if let Overlay::Sub { sel, .. } = &mut d.app.overlay {
                *sel = i;
            }
            assert!(d.shows(&label), "{label} at {w}x{h}");
        }
        d.key(KeyCode::Esc);
        assert!(matches!(
            d.app.overlay,
            Overlay::Sub {
                sub: Sub::Writing,
                ..
            }
        ));
        d.key(KeyCode::Esc);
        assert!(matches!(d.app.overlay, Overlay::Menu { .. }));
        assert!(d.app.menu_rows().len() <= 10);
    }
}
#[test]
fn writingdeck_productivity_tick_finalizes_due_timer_after_flushing_actual_pending_input() {
    let mut d = metric_writer("metrics-due-pending", 100, 30);
    let clock = metric_clock(&mut d);
    metric_new_sprint(&mut d, 0, "1");
    d.key(KeyCode::Esc);
    d.app.paste("pending actual words ");
    assert_eq!(d.app.productivity.ledger.pending_count(), 1);
    metric_advance(&clock, 60000);
    d.app.tick_productivity();
    assert_eq!(d.app.productivity.ledger.pending_count(), 0);
    let s = d.app.productivity.sprint.as_ref().unwrap();
    assert_eq!(
        s.status,
        grimoire_core::productivity::SprintStatus::Completed
    );
    assert_eq!(s.progress, 3);
    assert_eq!(s.elapsed_ms, 60000);
    assert!(
        d.app
            .productivity
            .config
            .awards
            .iter()
            .any(|a| a.id == format!("sprint-{}", s.id))
    );
}
#[test]
fn writingdeck_productivity_reload_detects_un_ticked_timer_conflict_and_keeps_external_and_local_states()
 {
    let mut d = metric_writer("metrics-live-conflict", 100, 30);
    let clock = metric_clock(&mut d);
    metric_new_sprint(&mut d, 0, "2");
    let file = d.root.join(".grimoire/writingdeck/productivity.json");
    let mut external = d.app.productivity.config.clone();
    external.sprint.as_mut().unwrap().status = grimoire_core::productivity::SprintStatus::Stopped;
    let bytes = serde_json::to_vec_pretty(&external).unwrap();
    fs::write(&file, &bytes).unwrap();
    metric_advance(&clock, 5000);
    d.app.reload_productivity();
    assert_eq!(d.app.productivity.sprint.as_ref().unwrap().elapsed_ms, 5000);
    assert_eq!(
        d.app.productivity.sprint.as_ref().unwrap().status,
        grimoire_core::productivity::SprintStatus::Running
    );
    assert!(d.app.productivity.warning.contains("Conflicting"));
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert!(!d.app.try_quit());
    assert_eq!(fs::read(&file).unwrap(), bytes);
}
#[test]
fn writingdeck_productivity_midnight_cache_and_repeated_day_retrieve_frozen_assignments_without_guessing_history()
 {
    let mut d = metric_writer("metrics-civil-days", 100, 30);
    let clock = metric_clock(&mut d);
    d.app.run_action(palette::Action::DailyChallenges);
    let first = d.app.productivity.day.as_ref().unwrap().value.clone();
    metric_advance(&clock, 86_400_000);
    d.app.tick_productivity();
    assert_eq!(d.app.productivity.stats_date, "2026-10-26");
    assert!(d.app.productivity.summary.starts_with("2026-10-26"));
    d.app.run_action(palette::Action::DailyChallenges);
    assert_eq!(
        d.app.productivity.day.as_ref().unwrap().value.date,
        "2026-10-26"
    );
    d.key(KeyCode::Left);
    assert_eq!(d.app.productivity.day.as_ref().unwrap().value, first);
    d.key(KeyCode::Left);
    assert!(d.app.productivity.day.is_none());
    assert!(
        !d.root
            .join(".grimoire/writingdeck/challenges/2026-10-24.json")
            .exists()
    );
}
#[test]
fn writingdeck_productivity_challenge_sprint_uses_remaining_target_and_frozen_accepted_mode_and_footnote_policy()
 {
    use grimoire_core::{accounting::Mode, productivity::SprintStatus};
    let mut d = metric_writer("metrics-challenge-sprint", 100, 30);
    metric_clock(&mut d);
    d.app.run_action(palette::Action::DailyChallenges);
    d.key(KeyCode::Char('a'));
    d.key(KeyCode::Esc);
    d.app.paste(&"word ".repeat(20));
    d.ctrl('s');
    d.app.run_action(palette::Action::Productivity);
    d.key(KeyCode::Char('f'));
    d.key(KeyCode::Char('m'));
    assert_eq!(d.app.productivity.config.mode, Mode::Revision);
    metric_new_sprint(&mut d, 4, "99");
    let s = d.app.productivity.sprint.as_ref().unwrap();
    assert_eq!(s.target, 30);
    assert_eq!(s.mode, Some(Mode::Drafting));
    assert!(!s.include_footnotes);
    assert_eq!(s.challenge.as_ref().unwrap().0, "2026-10-25");
    d.key(KeyCode::Esc);
    d.app.paste(&"revision ".repeat(50));
    d.ctrl('s');
    assert_ne!(
        d.app.productivity.sprint.as_ref().unwrap().status,
        SprintStatus::Completed
    );
    d.app.run_action(palette::Action::Productivity);
    d.key(KeyCode::Char('m'));
    d.key(KeyCode::Char('m'));
    d.key(KeyCode::Esc);
    d.app.paste(&"draft ".repeat(30));
    d.ctrl('s');
    assert_eq!(
        d.app.productivity.sprint.as_ref().unwrap().status,
        SprintStatus::Completed
    );
    d.app.run_action(palette::Action::DailyChallenges);
    d.key(KeyCode::Char('c'));
    assert_eq!(
        d.app.productivity.day.as_ref().unwrap().value.assignments[0].evidence,
        Some(grimoire_core::challenges::Evidence::Measured)
    );
}
#[test]
fn writingdeck_productivity_unknown_challenge_day_is_preserved_while_original_prose_and_metrics_save()
 {
    let mut d = metric_writer("metrics-future-day", 100, 30);
    metric_clock(&mut d);
    let file = d
        .root
        .join(".grimoire/writingdeck/challenges/2026-10-25.json");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let bytes = b"{\"version\":999,\"future_assignment\":\"retain\"}";
    fs::write(&file, bytes).unwrap();
    d.app.run_action(palette::Action::DailyChallenges);
    assert!(d.app.productivity.day.is_none());
    d.key(KeyCode::Char('R'));
    assert_eq!(fs::read(&file).unwrap(), bytes);
    d.key(KeyCode::Esc);
    d.typed("source safe ");
    d.ctrl('s');
    assert_eq!(
        metric_totals(&d, grimoire_core::accounting::Role::Manuscript)
            .typed
            .added,
        2
    );
    assert!(d.app.try_quit());
    assert_eq!(fs::read(&file).unwrap(), bytes);
}
#[test]
fn writingdeck_productivity_oversize_word_gap_is_unavailable_not_a_fake_zero_or_complete_word_sprint()
 {
    let mut d = metric_writer("metrics-word-gap", 100, 30);
    metric_clock(&mut d);
    d.app.paste(&"word ".repeat(52_500));
    d.ctrl('s');
    assert_eq!(
        metric_value(&d, grimoire_core::productivity::Metric::InputAdded),
        None
    );
    assert!(d.app.productivity.summary.contains("unavailable"));
    metric_new_sprint(&mut d, 0, "1");
    d.key(KeyCode::Char('x'));
    d.key(KeyCode::Char('n'));
    d.key(KeyCode::Down);
    d.key(KeyCode::Enter);
    metric_field(&mut d, "3");
    assert_ne!(
        d.app.productivity.sprint.as_ref().unwrap().kind,
        grimoire_core::productivity::SprintKind::Words
    );
    assert!(
        d.app.msg.contains("requires complete recorded metrics"),
        "{}",
        d.app.msg
    );
}
#[test]
fn writingdeck_productivity_real_saved_finished_statuses_award_scene_and_chapter_references_without_copying_prose()
 {
    let mut d = metric_writer("metrics-finished-status", 100, 30);
    metric_clock(&mut d);
    let scenes: Vec<_> = d
        .app
        .project
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == grimoire_core::project::Kind::Scene && n.in_manuscript)
        .map(|(i, _)| i)
        .collect();
    for i in &scenes {
        d.app
            .run_action(palette::Action::Open(d.app.project.nodes[*i].path.clone()));
        d.app.run_action(palette::Action::Details);
        for _ in 0..3 {
            d.key(KeyCode::Right);
        }
        d.key(KeyCode::Enter);
        d.app.commit_saves();
        assert_eq!(
            d.app.project.nodes[*i].meta("status").as_deref(),
            Some("done")
        );
    }
    let awards: Vec<_> = d
        .app
        .productivity
        .config
        .awards
        .iter()
        .filter(|a| a.source.is_some())
        .collect();
    assert!(awards.len() > scenes.len());
    assert!(
        awards
            .iter()
            .all(|a| !a.measured && a.source.as_ref().unwrap().starts_with("manuscript"))
    );
    let count = awards.len();
    d.app.save();
    assert_eq!(
        d.app
            .productivity
            .config
            .awards
            .iter()
            .filter(|a| a.source.is_some())
            .count(),
        count
    );
}
#[test]
fn writingdeck_productivity_empty_collection_and_subjective_challenge_do_not_complete_lessons_20_or_21()
 {
    use crate::app::writingdeck::Page;
    let root = std::env::temp_dir().join(format!("wd-metrics-notutorial-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    grimoire_core::tutorial::create_demo(&root).unwrap();
    let mut d = Desk::open(root, 100, 30);
    metric_clock(&mut d);
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 20 });
    d.key(KeyCode::Enter);
    assert!(d.app.productivity.config.awards.is_empty());
    assert!(!d.app.deck.progress.completed.contains(&20));
    d.app
        .productivity
        .update(|s| {
            s.challenge_profile.categories = [grimoire_core::challenges::Category::Creative]
                .into_iter()
                .collect();
            Ok(())
        })
        .unwrap();
    d.app.overlay = Overlay::Deck(Page::Lesson { id: 21 });
    d.key(KeyCode::Enter);
    d.key(KeyCode::Char('a'));
    d.key(KeyCode::Char('x'));
    assert_eq!(
        d.app.productivity.day.as_ref().unwrap().value.assignments[0].evidence,
        Some(grimoire_core::challenges::Evidence::SelfReported)
    );
    assert!(!d.app.deck.progress.completed.contains(&21));
}

#[path = "spark_ui_tests.rs"]
mod spark_ui_tests;

#[path = "checks_ui_tests.rs"]
mod checks_ui_tests;

#[path = "lexicon_ui_tests.rs"]
#[cfg(target_os = "linux")]
mod lexicon_ui_tests;

#[path = "prose_ui_tests.rs"]
#[cfg(target_os = "linux")]
mod prose_ui_tests;
