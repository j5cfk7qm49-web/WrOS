//! The menu bar across the top of the screen, its dropdowns, and Tools' side
//! groups: the one way into everything. Esc lights the bar; the letters,
//! arrows and the mouse open a menu; the highlighted row's line of
//! explanation shows on the status line.
#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use super::*;
use crate::app::outline::DeskView;
use crate::app::shell::Category;
use crate::palette::{Side, Title};
use ratatui::buffer::Buffer;
use ratatui::style::Color;

mod dropdown;
mod info;
pub(super) mod panel;
mod progress;
mod tiles;
mod top;

pub(super) use dropdown::draw_dropdowns;
pub(crate) use info::{InfoBar, focus_warning};
pub(crate) use progress::{ProgressBar, ProgressState};
pub(crate) use top::TopBar;

/// No dropdown is taller than this many rows; a short screen scrolls them.
pub const MAX_ROWS: usize = 12;

/// One row of a dropdown.
#[derive(Debug, Clone, PartialEq)]
pub struct BarRow {
    pub label: String,
    /// The key that does the same, shown at the right ("^Z", "F11", "›").
    pub key: String,
    pub action: Action,
    /// Why the row can't be chosen right now; it shows dim and says so.
    pub dim: Option<&'static str>,
}

impl BarRow {
    fn new(label: impl Into<String>, key: impl Into<String>, action: Action) -> Self {
        Self {
            label: label.into(),
            key: key.into(),
            action,
            dim: None,
        }
    }

    fn dim_unless(mut self, ok: bool, why: &'static str) -> Self {
        if !ok {
            self.dim = Some(why);
        }
        self
    }
}

/// What the mouse can land on in the bar and its dropdowns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarHit {
    Tile(DeskView),
    Title(Title),
    Row(usize),
    SideRow(usize),
    /// A side group's box: whatever of the menu it covers is out of reach.
    Cover,
}

const WRITE_FIRST: &str = "Open a scene first: this works in the page you're writing.";

impl App {
    /// The rows of one menu, with the conditions rc12's menu had: writing rows
    /// appear with a scene open, Restore with something in the Trash, and so on.
    pub fn bar_rows(&self, title: Title) -> Vec<BarRow> {
        let m = self.mod_label();
        let writing = self.open.is_some();
        let write_key = |key| if self.desk_accepts_input() { "" } else { key };
        let key = |action: wros_sys::shortcuts::Action, default: &str| {
            self.deck
                .shortcuts
                .chord_for(action)
                .unwrap_or_else(|| default.to_string())
        };
        use wros_sys::shortcuts::Action as Shortcut;
        let rows = match title {
            Title::File => {
                let mut rows = vec![];
                if !self.conflicts().is_empty() {
                    rows.push(BarRow::new("Settle conflicts…", "", Action::Conflicts));
                }
                rows.extend([
                    BarRow::new("New project…", "", Action::NewBook),
                    BarRow::new("Open another project…", "", Action::OpenBook),
                    BarRow::new("Export…", "", Action::Export),
                    BarRow::new("Compile to Markdown", "", Action::Compile),
                    BarRow::new("Update the project map", "", Action::ProjectMap),
                    BarRow::new("Back up now", "", Action::BackUpNow),
                    BarRow::new("Writing sessions…", "", Action::Sessions),
                    BarRow::new("Save this session now", "", Action::SaveSession),
                ]);
                // Only while a synced book still keeps its history inside it.
                if self.history_in_book && self.cloud.is_some() {
                    rows.push(BarRow::new(
                        "Move writing history out…",
                        "",
                        Action::MoveHistoryOut,
                    ));
                }
                rows.push(BarRow::new(
                    "MLA 9 paper details…",
                    "",
                    Action::MlaPaperDetails,
                ));
                rows
            }
            Title::Edit if self.desk_accepts_input() => vec![
                BarRow::new("Undo", format!("{m}Z"), Action::Undo),
                BarRow::new("Redo", format!("{m}Y"), Action::Redo),
                BarRow::new("Paste", format!("{m}V"), Action::Paste),
                BarRow::new("Search the whole project…", "", Action::FindInBook),
            ],
            Title::Edit => {
                let mut rows = vec![
                    BarRow::new("Undo", format!("{m}Z"), Action::Undo)
                        .dim_unless(writing, WRITE_FIRST),
                    BarRow::new("Redo", format!("{m}Y"), Action::Redo)
                        .dim_unless(writing, WRITE_FIRST),
                    BarRow::new("Cut", format!("{m}X"), Action::Cut)
                        .dim_unless(writing, WRITE_FIRST),
                    BarRow::new("Copy", format!("{m}C"), Action::Copy)
                        .dim_unless(writing, WRITE_FIRST),
                    BarRow::new("Paste", format!("{m}V"), Action::Paste)
                        .dim_unless(writing, WRITE_FIRST),
                    BarRow::new("Select all", format!("{m}A"), Action::SelectAll)
                        .dim_unless(writing, WRITE_FIRST),
                ];
                if writing {
                    rows.push(BarRow::new(
                        "Find and replace…",
                        format!("{m}F"),
                        Action::FindInScene,
                    ));
                }
                rows.push(BarRow::new(
                    "Search the whole project…",
                    "/",
                    Action::FindInBook,
                ));
                if writing {
                    rows.push(BarRow::new("Italic", format!("{m}I"), Action::Italic));
                    rows.push(BarRow::new("Bold", format!("{m}B"), Action::Bold));
                }
                rows.push(BarRow::new(
                    "Insert footnote here",
                    "",
                    Action::InsertFootnote,
                ));
                rows
            }
            Title::View => {
                let mut rows = vec![BarRow::new(
                    if self.commonplace.side_visible && self.desk_view == DeskView::Write {
                        "Hide Outline beside Write"
                    } else {
                        "Outline beside Write"
                    },
                    "",
                    Action::OutlineBeside,
                )];
                if self.desk_view == DeskView::Write {
                    if writing || self.focus_mode {
                        rows.push(BarRow::new(
                            if self.focus_mode {
                                "Leave focus mode"
                            } else {
                                "Focus mode"
                            },
                            format!("{m}D"),
                            Action::FocusMode,
                        ));
                    }
                    rows.push(BarRow::new(
                        if self.deck.state.current().outline {
                            "Hide the outline"
                        } else {
                            "Show the outline"
                        },
                        key(Shortcut::Outline, ""),
                        Action::ToggleOutline,
                    ));
                    rows.push(BarRow::new(
                        if self.pins.state().sidebar_visible {
                            "Hide the pins sidebar"
                        } else {
                            "Show the pins sidebar"
                        },
                        key(Shortcut::Pins, ""),
                        Action::TogglePins,
                    ));
                    if writing {
                        rows.push(BarRow::new(
                            "Open a scene beside…",
                            "v",
                            Action::BesidePicker,
                        ));
                    }
                    rows.extend([
                        BarRow::new(
                            if self.views.secondary_visible {
                                "Hide the second pane"
                            } else {
                                "Show the second pane"
                            },
                            key(Shortcut::Secondary, ""),
                            Action::ToggleSecondary,
                        ),
                        BarRow::new("Choose second document…", "", Action::SecondaryPicker),
                    ]);
                }
                rows.extend([
                    BarRow::new("Themes…", "F9", Action::Themes),
                    BarRow::new("Workspaces…", "", Action::Workspaces),
                ]);
                rows
            }
            Title::Book => {
                let part = self.project.meta.part_noun();
                let section = self.essay_section_selected();
                let mut rows = vec![BarRow::new(
                    if section {
                        "New section…"
                    } else {
                        "New scene…"
                    },
                    write_key("n"),
                    Action::NewScene,
                )];
                if !self.project.meta.is_essay() {
                    rows.extend([
                        BarRow::new("New chapter…", write_key("c"), Action::NewChapter),
                        BarRow::new(format!("New {part}…"), write_key("p"), Action::NewPart),
                    ]);
                }
                if !section {
                    rows.push(BarRow::new(
                        "New folder…",
                        write_key("N"),
                        Action::NewFolder,
                    ));
                }
                rows.extend([
                    BarRow::new("Rename…", "r", Action::Rename),
                    BarRow::new(
                        if self.visible.get(self.sel).is_some_and(|&i| {
                            self.project.nodes[i].path
                                == grimoire_core::project::trash_dir(&self.project.root)
                        }) {
                            "Empty the Trash…"
                        } else {
                            "Delete…"
                        },
                        "d",
                        Action::Delete,
                    ),
                ]);
                if self.selection_in_trash() {
                    rows.push(BarRow::new("Restore from the Trash", "u", Action::Restore));
                }
                rows.push(BarRow::new("Move up", "K", Action::MoveUp));
                rows.push(BarRow::new("Move down", "J", Action::MoveDown));
                if writing {
                    rows.push(BarRow::new("Scene details…", "", Action::Details));
                }
                if writing || !self.visible.is_empty() {
                    rows.push(BarRow::new("Scene history…", "H", Action::History));
                }
                rows.push(BarRow::new(
                    Side::ThisProject.label(),
                    "›",
                    Action::Side(Side::ThisProject),
                ));
                rows
            }
            Title::Tools => {
                let mut rows = vec![
                    if self.sprint.is_some() {
                        BarRow::new("Stop the sprint", "", Action::EndSprint)
                    } else {
                        BarRow::new("Start a sprint…", "", Action::StartSprint)
                    },
                    BarRow::new(
                        if self.pomo.running() {
                            "Pause the timer"
                        } else {
                            "Start the timer"
                        },
                        "F2",
                        Action::Timer,
                    ),
                    BarRow::new("Reset the timer", "F3", Action::TimerReset),
                ];
                if writing {
                    rows.push(BarRow::new("Read aloud", "", Action::ReadAloud));
                    rows.push(BarRow::new(
                        "Read chapter aloud",
                        "",
                        Action::ReadChapterAloud,
                    ));
                    rows.push(BarRow::new(
                        "Read manuscript aloud",
                        "",
                        Action::ReadManuscriptAloud,
                    ));
                }
                rows.push(BarRow::new("Music player…", "F7", Action::MusicPlayer));
                rows.push(BarRow::new(
                    "Dictionary and thesaurus…",
                    "",
                    Action::Lexicon,
                ));
                rows.push(BarRow::new("Writing checks…", "", Action::WritingChecks));
                for side in [Side::Revision, Side::Notes, Side::Practice] {
                    rows.push(BarRow::new(side.label(), "›", Action::Side(side)));
                }
                rows
            }
            Title::Settings => [
                (Category::Writing, "Writing"),
                (Category::Interface, "Look and keyboard"),
                (Category::System, "System"),
                (Category::Accounts, "Backups and accounts"),
                (Category::Updates, "Updates and recovery"),
                (Category::Music, "Music"),
            ]
            .into_iter()
            .map(|(group, label)| BarRow::new(label, "›", Action::SettingsGroup(group)))
            .collect(),
            Title::Help => vec![
                BarRow::new("Help…", "?", Action::Help),
                BarRow::new("Interactive tutorial…", "", Action::Tutorial),
                BarRow::new("License…", "", Action::License),
                BarRow::new("Donate…", "", Action::Donate),
                BarRow::new("About WrOS", "", Action::About),
            ],
            Title::Power => {
                let mut rows = vec![];
                if self.deck.live_session {
                    rows.push(
                        BarRow::new("Install WrOS to disk…", "", Action::InstallToDisk).dim_unless(
                            self.deck.installer_available,
                            "The installer can't run on this console or image.",
                        ),
                    );
                }
                rows.push(if self.deck.live_session {
                    BarRow::new("Lock", "", Action::Lock)
                        .dim_unless(false, "A live session has no personal lock.")
                } else if self.deck.graphical_session && !self.deck.lock_available {
                    BarRow::new("Lock", "", Action::Lock)
                        .dim_unless(false, "Lock isn't in the graphical session yet.")
                } else {
                    BarRow::new("Lock", "", Action::Lock).dim_unless(
                        self.deck.lock_available,
                        "Lock is unavailable in this image or session.",
                    )
                });
                rows.push(
                    BarRow::new("Sleep", "", Action::Sleep)
                        .dim_unless(self.deck.suspend_available, wros_sys::SUSPEND_UNAVAILABLE),
                );
                rows.push(BarRow::new("Restart…", "", Action::Restart));
                rows.push(BarRow::new("Shut down…", "", Action::ShutDown));
                if self.deck.quit_offered {
                    rows.push(BarRow::new("Quit", format!("{m}Q"), Action::Quit));
                }
                rows
            }
        };
        // #125: no silent cut. A menu that outgrows the cap fails the twelve-row test (and any
        // debug build) instead of losing its last row; a short screen still scrolls.
        debug_assert!(
            rows.len() <= MAX_ROWS,
            "{} menu has {} rows (most {MAX_ROWS})",
            title.label(),
            rows.len()
        );
        self.scope_rows(rows)
    }

    /// The rows of one of Tools' side groups.
    pub fn side_rows(&self, side: Side) -> Vec<BarRow> {
        let m = self.mod_label();
        let writing = self.open.is_some();
        let rows = match side {
            Side::ThisProject => vec![
                BarRow::new("Where this project is kept…", "", Action::WhereKept),
                BarRow::new("Save a copy (one file)", "", Action::SaveCopy),
                BarRow::new("Goals…", "", Action::Goals),
            ],
            Side::Revision => {
                let mut rows = vec![BarRow::new(
                    "Notes and TKs…",
                    format!("{m}T"),
                    Action::NotesList,
                )];
                if writing {
                    rows.push(BarRow::new("Next TK or note", "", Action::NextTk));
                }
                rows.push(BarRow::new(
                    "Next scene still in draft",
                    "",
                    Action::NextDraft,
                ));
                if writing || self.echo_on {
                    rows.push(BarRow::new(
                        if self.echo_on {
                            "Turn echo words off"
                        } else {
                            "Turn echo words on"
                        },
                        "",
                        Action::EchoWords,
                    ));
                }
                rows.extend([
                    BarRow::new("Check names…", "", Action::CheckNames),
                    BarRow::new(
                        "Corkboard…",
                        if self.desk_accepts_input() { "" } else { "b" },
                        Action::Corkboard,
                    ),
                    BarRow::new("Footnote navigator…", "", Action::Footnotes),
                    BarRow::new("Read footnote at caret…", "", Action::FootnoteAtCaret),
                    BarRow::new("Continuity tracker…", "", Action::Continuity),
                    BarRow::new("Draft milestones…", "", Action::Milestones),
                    BarRow::new("Revision notes…", "", Action::ReviewNotes),
                    BarRow::new("Prose and revision dashboard…", "", Action::ProseDashboard),
                ]);
                rows
            }
            Side::Notes => vec![
                BarRow::new("Pinboard…", "", Action::Pinboard),
                BarRow::new("Pin this document", "", Action::PinCurrent),
                BarRow::new("Writing journal…", "", Action::Journal),
                BarRow::new("Scratchpad…", "", Action::Scratchpad),
                BarRow::new("Session handoff…", "", Action::Handoff),
                BarRow::new("Project sources…", "", Action::ProjectSources),
                BarRow::new("Add a source…", "", Action::AddProjectSource),
                BarRow::new("Import sources…", "", Action::ImportProjectSources),
                BarRow::new(
                    "Project sources timeline…",
                    "",
                    Action::ProjectSourcesTimeline,
                ),
                BarRow::new(
                    "Resolve source conflict…",
                    "",
                    Action::ProjectSourcesConflict,
                ),
                BarRow::new("Look up the name here", format!("{m}O"), Action::OpenCodex)
                    .dim_unless(writing, WRITE_FIRST),
                BarRow::new(
                    "read",
                    "Super+2",
                    Action::SwitchDeskView(crate::app::outline::DeskView::Read),
                ),
            ],
            Side::Practice => vec![
                BarRow::new("Progress…", "", Action::Progress),
                BarRow::new("Goals…", "", Action::MeasuredGoals),
                BarRow::new("Calendar…", "", Action::Calendar),
                BarRow::new("Sprints…", "", Action::AdvancedSprints),
                BarRow::new("Edits…", "", Action::Productivity),
                BarRow::new("Collection…", "", Action::Achievements),
                BarRow::new("Challenges…", "", Action::DailyChallenges),
                BarRow::new("Story Spark…", "", Action::StorySpark),
            ],
        };
        self.scope_rows(rows)
    }

    fn scope_rows(&self, mut rows: Vec<BarRow>) -> Vec<BarRow> {
        for row in &mut rows {
            if let Some(reason) = self.unavailable_action(&row.action) {
                row.dim = Some(reason);
                row.key.clear();
            }
        }
        rows
    }

    /// Where `action` lives on the bar, as "Tools › Notes" (Find anything
    /// shows it, so searching teaches the menus).
    pub fn bar_path(&self, action: &Action) -> Option<String> {
        for title in Title::ALL {
            for row in self.bar_rows(title) {
                if row.action == *action {
                    return Some(title.label().to_string());
                }
                if let Action::Side(side) = row.action
                    && self.side_rows(side).iter().any(|r| r.action == *action)
                {
                    return Some(format!("{} › {}", title.label(), side.label()));
                }
            }
        }
        None
    }

    /// Light the bar on `title` (or the one used last), its menu dropped down
    /// if `open`.
    pub fn open_bar(&mut self, title: Option<Title>, open: bool) {
        let title = title.unwrap_or(self.bar_last);
        self.bar_last = title;
        self.overlay = Overlay::Bar {
            title,
            tile: None,
            open,
            sel: 0,
            side: None,
        };
        self.observe_deck(grimoire_core::tutorial::Event::MenuBarOpened);
    }

    /// Tools, with one of its side groups open beside it.
    pub fn open_side(&mut self, side: Side) {
        let title = if side == Side::ThisProject {
            Title::Book
        } else {
            Title::Tools
        };
        let sel = self
            .bar_rows(title)
            .iter()
            .position(|r| r.action == Action::Side(side))
            .unwrap_or(0);
        self.bar_last = title;
        self.overlay = Overlay::Bar {
            title,
            tile: None,
            open: true,
            sel,
            side: Some((side, 0)),
        };
    }

    fn select_bar_tile(&mut self, view: DeskView) {
        self.overlay = Overlay::Bar {
            title: Title::File,
            tile: Some(view),
            open: false,
            sel: 0,
            side: None,
        };
    }

    fn activate_bar_tile(&mut self, view: DeskView) {
        self.close_bar();
        self.switch_desk_view(view);
    }

    pub fn close_bar(&mut self) {
        if matches!(self.overlay, Overlay::Bar { .. }) {
            self.overlay = Overlay::None;
        }
    }

    /// The rows of whichever list is open: a side group, or the menu.
    #[cfg(feature = "test-driver")]
    pub fn bar_open_rows(&self) -> Vec<BarRow> {
        match &self.overlay {
            Overlay::Bar {
                side: Some((side, _)),
                ..
            } => self.side_rows(*side),
            Overlay::Bar {
                title, open: true, ..
            } => self.bar_rows(*title),
            _ => vec![],
        }
    }

    /// The highlighted row, while a menu is open.
    pub fn bar_highlight(&self) -> Option<BarRow> {
        let Overlay::Bar {
            title,
            open: true,
            sel,
            side,
            ..
        } = &self.overlay
        else {
            return None;
        };
        match side {
            Some((side, i)) => self.side_rows(*side).into_iter().nth(*i),
            None => self.bar_rows(*title).into_iter().nth(*sel),
        }
    }

    /// The highlighted row's line of explanation (or why it's dim) and its
    /// help topic: the status line shows it while a menu is open.
    pub fn bar_description(&self) -> Option<(String, &'static str)> {
        let row = self.bar_highlight()?;
        let (says, topic) = super::menu::about_row(&row.action, &self.project.meta.part_noun());
        Some((row.dim.map_or(says, str::to_string), topic))
    }

    pub(super) fn on_bar_key(&mut self, key: Key) {
        let Overlay::Bar {
            title,
            tile,
            open,
            sel,
            side,
        } = self.overlay.clone()
        else {
            return;
        };
        if key == Key::Char('?') {
            self.open_help(None);
            return;
        }
        // The menu-bar key closes it again.
        if let Key::F(n) = key
            && self
                .deck
                .shortcuts
                .chord_for(wros_sys::shortcuts::Action::MenuBar)
                == Some(format!("F{n}"))
        {
            self.close_bar();
            return;
        }
        if let Some(view) = tile {
            match key {
                Key::Left | Key::Right => {
                    let Some(at) = DeskView::ALL.iter().position(|&v| v == view) else {
                        return;
                    };
                    let next = if key == Key::Left {
                        at.saturating_sub(1)
                    } else {
                        at + 1
                    };
                    if let Some(&view) = DeskView::ALL.get(next) {
                        self.select_bar_tile(view);
                    } else {
                        self.open_bar(Some(Title::File), false);
                    }
                    return;
                }
                Key::Enter | Key::Char(' ') => {
                    self.activate_bar_tile(view);
                    return;
                }
                Key::Down => return,
                _ => {}
            }
        } else if title == Title::File && key == Key::Left {
            self.select_bar_tile(DeskView::Write);
            return;
        }
        if !open {
            match key {
                Key::Left => self.open_bar(Some(title.step(-1)), false),
                Key::Right => self.open_bar(Some(title.step(1)), title == Title::Help),
                Key::Down | Key::Enter | Key::Char(' ') => self.open_bar(Some(title), true),
                Key::Esc => self.close_bar(),
                Key::Char(c) => {
                    if let Some(t) = Title::for_letter(c) {
                        self.open_bar(Some(t), true);
                    }
                }
                _ => {}
            }
            return;
        }
        if let Some((group, mut at)) = side {
            let rows = self.side_rows(group);
            if list_nav(key, &mut at, rows.len(), RING) {
                self.set_bar_side(Some((group, at)));
                return;
            }
            match key {
                Key::Esc | Key::Left => self.set_bar_side(None),
                Key::Right => self.open_bar(Some(title.step(1)), true),
                Key::Enter | Key::Char(' ') => {
                    if let Some(row) = rows.get(at).cloned() {
                        self.run_bar_row(row);
                    }
                }
                Key::Char(c) => {
                    if let Some(i) = jump(&rows, at, c) {
                        self.set_bar_side(Some((group, i)));
                    }
                }
                _ => {}
            }
            return;
        }
        let rows = self.bar_rows(title);
        let mut at = sel;
        if list_nav(key, &mut at, rows.len(), RING) {
            self.set_bar_sel(at);
            return;
        }
        match key {
            Key::Esc => self.close_bar(),
            Key::Left => self.open_bar(Some(title.step(-1)), true),
            Key::Right => match rows.get(sel).map(|r| &r.action) {
                Some(Action::Side(group)) => self.set_bar_side(Some((*group, 0))),
                _ => self.open_bar(Some(title.step(1)), true),
            },
            Key::Enter | Key::Char(' ') => {
                if let Some(row) = rows.get(sel).cloned() {
                    self.run_bar_row(row);
                }
            }
            Key::Char(c) => {
                if let Some(i) = jump(&rows, sel, c) {
                    self.set_bar_sel(i);
                }
            }
            _ => {}
        }
    }

    fn set_bar_sel(&mut self, to: usize) {
        if let Overlay::Bar { sel, side, .. } = &mut self.overlay {
            *sel = to;
            *side = None;
        }
    }

    fn set_bar_side(&mut self, to: Option<(Side, usize)>) {
        if let Overlay::Bar { side, .. } = &mut self.overlay {
            *side = to;
        }
    }

    /// Choose a row: a side group opens beside it; anything else closes the
    /// menu and runs. A setting switched on or off keeps the menu open so the
    /// row shows its new state.
    fn run_bar_row(&mut self, row: BarRow) {
        if let Some(why) = row.dim {
            self.msg = why.into();
            return;
        }
        if let Action::Side(group) = row.action {
            self.set_bar_side(Some((group, 0)));
            return;
        }
        let back = self.overlay.clone();
        let stay = row.action.is_setting_toggle() || row.action == Action::EchoWords;
        self.overlay = Overlay::None;
        self.run_action(row.action);
        if stay && self.overlay == Overlay::None {
            self.overlay = back;
        }
    }

    /// A click while the bar is showing. A tile switches views, a title toggles
    /// its menu, a row runs, and a click elsewhere closes an open menu.
    pub fn bar_click(&mut self, x: u16, y: u16) -> bool {
        let hit = self
            .bar_hits
            .borrow()
            .iter()
            .rev() // what's drawn last is on top
            .find(|(r, _)| hit(*r, x, y))
            .map(|(_, h)| *h);
        let up = matches!(self.overlay, Overlay::Bar { .. });
        match (hit, &self.overlay) {
            (Some(BarHit::Tile(view)), Overlay::None | Overlay::Bar { .. }) => {
                self.activate_bar_tile(view);
                true
            }
            (
                Some(BarHit::Title(t)),
                Overlay::Bar {
                    title, open: true, ..
                },
            ) if *title == t => {
                self.close_bar();
                true
            }
            (Some(BarHit::Title(t)), Overlay::None | Overlay::Bar { .. }) => {
                self.open_bar(Some(t), true);
                true
            }
            (Some(BarHit::Row(i)), Overlay::Bar { title, .. }) => {
                let title = *title;
                self.set_bar_sel(i);
                if let Some(row) = self.bar_rows(title).get(i).cloned() {
                    self.run_bar_row(row);
                }
                true
            }
            (
                Some(BarHit::SideRow(i)),
                Overlay::Bar {
                    side: Some((group, _)),
                    ..
                },
            ) => {
                let group = *group;
                self.set_bar_side(Some((group, i)));
                if let Some(row) = self.side_rows(group).get(i).cloned() {
                    self.run_bar_row(row);
                }
                true
            }
            (Some(BarHit::Cover), _) => true,
            _ if up => {
                self.close_bar();
                true
            }
            _ => false,
        }
    }

    /// The pointer moving with a menu open: across the titles switches menus,
    /// over a row highlights it.
    pub fn bar_hover(&mut self, x: u16, y: u16) {
        let Overlay::Bar {
            title,
            open: true,
            side,
            ..
        } = self.overlay.clone()
        else {
            return;
        };
        let hit = self
            .bar_hits
            .borrow()
            .iter()
            .rev() // what's drawn last is on top
            .find(|(r, _)| hit(*r, x, y))
            .map(|(_, h)| *h);
        match hit {
            Some(BarHit::Title(t)) if t != title => self.open_bar(Some(t), true),
            Some(BarHit::Row(i)) => {
                let keeps_side = side.is_some_and(|(group, _)| {
                    self.bar_rows(title).get(i).map(|r| &r.action) == Some(&Action::Side(group))
                });
                if !keeps_side {
                    self.set_bar_sel(i);
                }
            }
            Some(BarHit::SideRow(i)) => {
                if let Some((group, _)) = side {
                    self.set_bar_side(Some((group, i)));
                }
            }
            _ => {}
        }
    }

    /// The wheel over an open menu moves its highlight. True if it did.
    pub fn bar_scroll(&mut self, down: bool) -> bool {
        if !matches!(self.overlay, Overlay::Bar { open: true, .. }) {
            return false;
        }
        self.on_bar_key(if down { Key::Down } else { Key::Up });
        true
    }
}

/// The next row after `from` whose label starts with `c`, going round.
fn jump(rows: &[BarRow], from: usize, c: char) -> Option<usize> {
    let c = c.to_lowercase().next()?;
    (1..=rows.len())
        .map(|step| (from + step) % rows.len())
        .find(|&i| rows[i].label.to_lowercase().starts_with(c))
}

fn hit(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

/// The ground of the bars, in the same colour as the pane borders. None on the console,
/// whose sixteen colours would turn it into a bright block.
pub(crate) fn bar_band(t: &Theme, linux_console: bool) -> Option<Color> {
    (!linux_console && matches!(t.border, Color::Rgb(..))).then_some(t.border)
}

/// Lays `band` under every cell of `area` that has no ground of its own, so a lit title or a
/// highlighted mark keeps its shade.
fn paint_band(buf: &mut Buffer, area: Rect, t: &Theme, console: bool) {
    let Some(band) = bar_band(t, console) else {
        return;
    };
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            if cell.bg == Color::Reset {
                cell.set_bg(band);
            }
        }
    }
}

fn ellipsis(console: bool) -> &'static str {
    if console { "..." } else { "…" }
}

/// Shorten at a character boundary using terminal cells, including wide titles.
fn fit_text(text: &str, width: usize, console: bool) -> String {
    if Span::raw(text).width() <= width {
        return text.to_owned();
    }
    let ellipsis = ellipsis(console);
    let Some(room) = width.checked_sub(Span::raw(ellipsis).width()) else {
        return ".".repeat(width);
    };
    let end = text
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| Span::raw(&text[..i]).width() <= room)
        .last()
        .unwrap_or_default();
    format!("{}{ellipsis}", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_jumps_round_to_the_next_row_starting_with_it() {
        let rows: Vec<BarRow> = ["Start", "Save", "Open", "Sync"]
            .into_iter()
            .map(|l| BarRow::new(l, "", Action::Help))
            .collect();
        assert_eq!(jump(&rows, 0, 's'), Some(1));
        assert_eq!(jump(&rows, 1, 'S'), Some(3));
        assert_eq!(jump(&rows, 3, 's'), Some(0));
        assert_eq!(jump(&rows, 0, 'x'), None);
    }
}
