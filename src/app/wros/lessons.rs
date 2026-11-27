use super::{Deck, Page};
use crate::app::{App, Focus, Overlay, productivity, prose_dashboard, shell};
use crate::palette::Action;
use grimoire_core::{project, tutorial};
use std::fs;

impl Deck {
    fn observe(&mut self, event: tutorial::Event, demo: bool) {
        if self.progress.observe(event, demo) {
            self.note = "Tutorial exercise verified in the actual application.".into();
            self.save_progress();
        }
    }
    pub(super) fn save_progress(&mut self) {
        if let Some(store) = &mut self.tutorial_store {
            store.progress = self.progress.clone();
            if let Err(e) = store.save() {
                self.note = format!("Tutorial progress not saved: {e}");
            }
        }
    }
}

impl App {
    pub(crate) fn observe_deck(&mut self, event: tutorial::Event) {
        self.deck
            .observe(event, tutorial::is_demo(&self.project.root));
    }

    /// `d` on the tutorial page: open the safe demo book, making it only the first time.
    /// Lessons already done or skipped carry into it, so the count does not start over.
    pub(super) fn start_tutorial_demo(&mut self) {
        if tutorial::is_demo(&self.project.root) {
            self.deck.note = format!(
                "You're in the demo project {} ({}). Choose a lesson and press Enter.",
                crate::books::title_of_folder(&self.project.root),
                crate::books::pretty(&self.project.root)
            );
            return;
        }
        let parent = if self.background {
            grimoire_core::paths::data_dir().join("writingdeck")
        } else if let Some(parent) = self.project.root.parent() {
            parent.to_path_buf()
        } else {
            self.deck.note = "The current project has no parent folder".into();
            return;
        };
        if let Err(e) = fs::create_dir_all(&parent) {
            self.deck.note = e.to_string();
            return;
        }
        let carried = self.deck.progress.clone();
        let (root, made) = match existing_demo(&parent) {
            Some(root) => (root, false),
            None => {
                let mut n = 1;
                let mut root = parent.join("Tutorial-1");
                while root.exists() {
                    n += 1;
                    root = parent.join(format!("Tutorial-{n}"));
                }
                if let Err(e) = tutorial::create_demo(&root) {
                    self.deck.note = format!("Demo project not created: {e:#}");
                    return;
                }
                (root, true)
            }
        };
        if !self.open_book(&root) {
            return;
        }
        let progress = &mut self.deck.progress;
        let before = (progress.completed.len(), progress.skipped.len());
        progress.completed.extend(carried.completed_lessons());
        for id in carried.skipped {
            if !progress.completed.contains(&id) {
                progress.skipped.insert(id);
            }
        }
        if before != (progress.completed.len(), progress.skipped.len()) {
            self.deck.save_progress();
        }
        self.overlay = Overlay::Deck(Page::Tutorial { sel: 0 });
        self.deck.note = format!(
            "{} the demo project {} ({}). Your project was saved and left untouched.",
            if made { "Made and opened" } else { "Opened" },
            crate::books::title_of_folder(&root),
            crate::books::pretty(&root)
        );
    }
    pub(super) fn start_deck_lesson(&mut self, id: u8) {
        use shell::{Panel, Route};
        if tutorial::lessons()
            .iter()
            .find(|l| l.id == id)
            .and_then(|l| l.event)
            .is_some_and(|e| !e.safe_outside_demo())
            && !tutorial::is_demo(&self.project.root)
        {
            self.deck.note = "This exercise needs the safe demo project: press d in Interactive \
                              tutorial first. Your project is unchanged."
                .into();
            return;
        }
        if let Err(e) = self.deck.progress.start(id) {
            self.deck.note = e.to_string();
            return;
        }
        self.deck.save_progress();
        match id {
            1 => self.open_menu(),
            2 => self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 }),
            3 => {
                self.overlay = Overlay::NewBook {
                    buf: String::new(),
                    shape: project::Template::Novel,
                };
            }
            4 => {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
            }
            5 => {
                self.overlay = Overlay::None;
                self.deck.state.current_mut().outline = true;
                self.focus_mode = false;
                self.focus = Focus::Tree;
            }
            6 => self.run_action(Action::Corkboard),
            7 => {
                self.overlay = Overlay::Deck(Page::Workspaces {
                    sel: self.deck.state.selected,
                })
            }
            8 => {
                self.overlay = Overlay::None;
                if !self.focus_mode {
                    self.toggle_focus_mode();
                }
            }
            9 => {
                self.overlay = Overlay::None;
                self.toggle_outline();
            }
            10 => {
                self.overlay = Overlay::Deck(Page::Shortcuts {
                    sel: 0,
                    capture: false,
                })
            }
            11 => self.pin_picker(None),
            12 => self.open_pinboard(),
            13 => {
                self.focus_mode = false;
                self.open_secondary_picker();
            }
            14 => {
                self.journal.review_lesson_start();
                self.open_journal();
            }
            15 => self.open_scratchpad(),
            16 => self.open_handoff(),
            17 => self.run_action(Action::Goals),
            18 => self.open_productivity(productivity::Page::Calendar),
            19 => self.run_action(Action::StartSprint),
            20 => self.open_productivity(productivity::Page::Achievements),
            21 => self.open_productivity(productivity::Page::Challenges),
            22..=24 => self.open_spark(),
            25 => self.open_lexicon(),
            27..=29 => self.open_prose_dashboard(),
            30 => {
                self.open_prose_dashboard();
                self.prose_dashboard.page = prose_dashboard::Page::Voice;
            }
            26 => {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
                self.spelling();
            }
            40 => {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
                self.msg = "Optional exercise: type an article mistake in this demo manuscript, then open Writing checks and read its model disclosure. No permission or text has been inserted.".into();
            }
            31 => self.open_continuity(),
            32 => self.open_review_notes(),
            33 => {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
                self.open_history();
            }
            34 | 35 => self.run_action(Action::Export),
            // Pages by name; the Settings menu map knows where each one sits.
            36 | 37 => self.open_settings_page(Route::Backups),
            41 => self.open_settings_page(Route::GoogleAccount),
            42 => {
                let mut path = shell::path_to(Route::Panel(Panel::Display));
                path.push(Route::WarmScreen);
                self.open_settings_at(&path);
            }
            43 => self.open_settings_page(Route::Panel(Panel::Updates)),
            44 => self.open_settings_page(Route::Panel(Panel::Network)),
            45..=47 => self.open_settings_page(Route::Panel(Panel::Power)),
            48 => self.open_settings_page(Route::GoogleAccount),
            49 => self.open_merge_practice(),
            50 => self.open_settings_page(Route::Panel(Panel::Display)),
            38 => self.open_system_settings(),
            39 => {
                self.open_help(Some("history"));
                self.observe_deck(tutorial::Event::RecoveryHelpOpened);
            }
            _ => {}
        }
    }
}

/// The newest safe demo book in `parent` (Tutorial-1, Tutorial-2, …), if one is there.
fn existing_demo(parent: &std::path::Path) -> Option<std::path::PathBuf> {
    fs::read_dir(parent)
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let n: u32 = path
                .file_name()?
                .to_str()?
                .strip_prefix("Tutorial-")?
                .parse()
                .ok()?;
            let real = !path.symlink_metadata().ok()?.file_type().is_symlink();
            (real && tutorial::is_demo(&path)).then_some((n, path))
        })
        .max_by_key(|(n, _)| *n)
        .map(|(_, path)| path)
}
