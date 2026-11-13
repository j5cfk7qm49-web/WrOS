//! The menu bar across the top of the screen, its dropdowns, and Tools' side
//! groups: the one way into everything. Esc lights the bar; the letters,
//! arrows and the mouse open a menu; the highlighted row's line of
//! explanation shows on the status line.
use super::*;
use crate::app::shell::Category;
use crate::palette::{Side, Title};

/// No dropdown is taller than this many rows; a short screen scrolls them.
pub const MAX_ROWS: usize = 12;

/// One row of a dropdown.
#[derive(Debug, Clone, PartialEq)]
pub struct BarRow {
    pub label: String,
    /// The key that does the same, shown at the right ("^Z", "F12", "›").
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
    Title(Title),
    Row(usize),
    SideRow(usize),
}

const WRITE_FIRST: &str = "Open a scene first: this works in the page you're writing.";

impl App {
    /// The rows of one menu, with the conditions rc12's menu had: writing rows
    /// appear with a scene open, Restore with something in the Trash, and so on.
    pub fn bar_rows(&self, title: Title) -> Vec<BarRow> {
        let m = self.mod_label();
        let writing = self.open.is_some();
        let key = |action: wros_sys::shortcuts::Action, default: &str| {
            self.deck
                .shortcuts
                .chord_for(action)
                .unwrap_or_else(|| default.to_string())
        };
        use wros_sys::shortcuts::Action as Shortcut;
        let mut rows = match title {
            Title::Wros => {
                let mut rows = vec![
                    BarRow::new("Overview", key(Shortcut::Dashboard, ""), Action::Dashboard),
                    BarRow::new("About WrOS", "", Action::About),
                ];
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
            Title::File => {
                let mut rows = vec![];
                if !self.conflicts().is_empty() {
                    rows.push(BarRow::new("Settle conflicts…", "", Action::Conflicts));
                }
                rows.extend([
                    BarRow::new("Start a new book…", "", Action::NewBook),
                    BarRow::new("Open another book…", "", Action::OpenBook),
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
                rows
            }
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
                    "Search the whole book…",
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
                let mut rows = vec![];
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
                    rows.push(BarRow::new(
                        "Look up the name here",
                        format!("{m}O"),
                        Action::OpenCodex,
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
                    BarRow::new("Corkboard…", "b", Action::Corkboard),
                    BarRow::new("Themes…", "F9", Action::Themes),
                    BarRow::new("Workspaces…", "", Action::Workspaces),
                ]);
                rows
            }
            Title::Book => {
                let part = self.project.meta.part_noun();
                let mut rows = vec![
                    BarRow::new("New scene…", "n", Action::NewScene),
                    BarRow::new("New chapter…", "c", Action::NewChapter),
                    BarRow::new(format!("New {part}…"), "p", Action::NewPart),
                    BarRow::new("New folder…", "N", Action::NewFolder),
                    BarRow::new("Rename…", "r", Action::Rename),
                    BarRow::new("Delete…", "d", Action::Delete),
                ];
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
                rows.push(BarRow::new("Goals…", "", Action::Goals));
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
                if self.music.enabled {
                    rows.push(BarRow::new("Music player…", "F7", Action::MusicPlayer));
                }
                rows.push(BarRow::new(
                    "Dictionary and thesaurus…",
                    "",
                    Action::Lexicon,
                ));
                rows.push(BarRow::new("Writing checks…", "", Action::WritingChecks));
                for side in Side::ALL {
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
            ]
            .into_iter()
            .map(|(group, label)| BarRow::new(label, "›", Action::SettingsGroup(group)))
            .collect(),
            Title::Help => vec![
                BarRow::new("Help…", "?", Action::Help),
                BarRow::new("Interactive tutorial…", "", Action::Tutorial),
                BarRow::new("License…", "", Action::License),
                BarRow::new("Donate…", "", Action::Donate),
            ],
        };
        rows.truncate(MAX_ROWS);
        rows
    }

    /// The rows of one of Tools' side groups.
    pub fn side_rows(&self, side: Side) -> Vec<BarRow> {
        let m = self.mod_label();
        let writing = self.open.is_some();
        match side {
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
            ],
            Side::Practice => vec![
                BarRow::new("Story Spark…", "", Action::StorySpark),
                BarRow::new("Daily challenges…", "", Action::DailyChallenges),
                BarRow::new("Progress…", "", Action::Progress),
                BarRow::new("Recorded edit ledger…", "", Action::Productivity),
                BarRow::new("Advanced goals…", "", Action::MeasuredGoals),
                BarRow::new("Calendar and rest days…", "", Action::Calendar),
                BarRow::new("Advanced sprints…", "", Action::AdvancedSprints),
                BarRow::new("Milestone collection…", "", Action::Achievements),
            ],
        }
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
            open,
            sel: 0,
            side: None,
        };
    }

    /// Tools, with one of its side groups open beside it.
    pub fn open_side(&mut self, side: Side) {
        let sel = self
            .bar_rows(Title::Tools)
            .iter()
            .position(|r| r.action == Action::Side(side))
            .unwrap_or(0);
        self.bar_last = Title::Tools;
        self.overlay = Overlay::Bar {
            title: Title::Tools,
            open: true,
            sel,
            side: Some((side, 0)),
        };
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
        if !open {
            match key {
                Key::Left => self.open_bar(Some(title.step(-1)), false),
                Key::Right => self.open_bar(Some(title.step(1)), false),
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

    /// A click while the bar is showing. True if the bar took it: a title
    /// opens (or, open already, closes), a row runs, and anywhere else closes
    /// an open menu without reaching what's under it.
    pub fn bar_click(&mut self, x: u16, y: u16) -> bool {
        let hit = self
            .bar_hits
            .borrow()
            .iter()
            .find(|(r, _)| hit(*r, x, y))
            .map(|(_, h)| *h);
        let up = matches!(self.overlay, Overlay::Bar { .. });
        match (hit, &self.overlay) {
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

/// The bar's line: the titles, with the one in use lit. Records where each
/// title is for the mouse.
pub fn draw_bar_line(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let lit = match &app.overlay {
        Overlay::Bar { title, .. } => Some(*title),
        Overlay::Palette {
            under_bar: true, ..
        } => Some(app.bar_last),
        _ => None,
    };
    let look: wros_shell::Theme = t.into();
    let mut spans = vec![Span::raw(" ")];
    let mut x = area.x + 1;
    let mut hits = app.bar_hits.borrow_mut();
    hits.retain(|(_, h)| !matches!(h, BarHit::Title(_)));
    for title in Title::ALL {
        let parts = wros_shell::look::underlined_title(title.label(), 0, lit == Some(title), look);
        let w = title.label().chars().count() as u16 + 2;
        if x + w > area.x + area.width {
            break;
        }
        hits.push((Rect::new(x, area.y, w, 1), BarHit::Title(title)));
        x += w;
        spans.extend(parts);
    }
    f.render_widget(Clear, Rect::new(area.x, area.y, x - area.x, 1));
    f.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(area.x, area.y, x - area.x, 1),
    );
}

/// The open menu, under its title, and a side group beside it.
pub(super) fn draw_dropdowns(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Bar {
        title,
        open: true,
        sel,
        side,
    } = &app.overlay
    else {
        return;
    };
    let anchor = app
        .bar_hits
        .borrow()
        .iter()
        .find(|(_, h)| *h == BarHit::Title(*title))
        .map_or(Rect::new(area.x, area.y, 1, 1), |(r, _)| *r);
    let rows = app.bar_rows(*title);
    let menu = draw_list(
        f,
        area,
        anchor.x,
        anchor.y + 1,
        &rows,
        *sel,
        side.is_none(),
        t,
    );
    let mut hits = app.bar_hits.borrow_mut();
    hits.retain(|(_, h)| matches!(h, BarHit::Title(_)));
    for (i, r) in &menu.rows {
        hits.push((*r, BarHit::Row(*i)));
    }
    if let Some((group, at)) = side {
        let side_rows = app.side_rows(*group);
        let row_y = menu
            .rows
            .iter()
            .find(|(i, _)| *i == *sel)
            .map_or(menu.area.y, |(_, r)| r.y - 1);
        let w = list_width(&side_rows);
        let x = if menu.area.right() + w <= area.right() {
            menu.area.right()
        } else {
            menu.area.x.saturating_sub(w)
        };
        let drawn = draw_list(f, area, x, row_y, &side_rows, *at, true, t);
        for (i, r) in drawn.rows {
            hits.push((r, BarHit::SideRow(i)));
        }
    }
}

struct Drawn {
    area: Rect,
    rows: Vec<(usize, Rect)>,
}

fn list_width(rows: &[BarRow]) -> u16 {
    let label = rows
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0);
    let key = rows
        .iter()
        .map(|r| r.key.chars().count())
        .max()
        .unwrap_or(0);
    // Border, marker, label, a gap, the key, a space, border.
    (1 + 2 + label + 3 + key + 1 + 1) as u16
}

/// One dropdown box at (x, y), clamped into `area`, scrolled so row `sel`
/// shows. `lit` draws the highlight (a menu whose side group is open keeps
/// its row shaded but not lit).
#[allow(clippy::too_many_arguments)]
fn draw_list(
    f: &mut Frame,
    area: Rect,
    x: u16,
    y: u16,
    rows: &[BarRow],
    sel: usize,
    lit: bool,
    t: &Theme,
) -> Drawn {
    let w = list_width(rows).min(area.width);
    // The status line stays visible under a menu.
    let room = area
        .bottom()
        .saturating_sub(1)
        .saturating_sub(y)
        .saturating_sub(2) as usize;
    let shown = rows.len().min(MAX_ROWS).min(room.max(1));
    let h = shown as u16 + 2;
    let x = x.min(area.right().saturating_sub(w));
    let y = y.min(area.bottom().saturating_sub(h));
    let rect = Rect::new(x, y, w, h);
    f.render_widget(Clear, rect);
    let block = pane_block("", true, t);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let start = (sel + 1).saturating_sub(shown);
    let look: wros_shell::Theme = t.into();
    let mut drawn = vec![];
    for (line, (i, row)) in rows.iter().enumerate().skip(start).take(shown).enumerate() {
        let on = i == sel;
        let base = if row.dim.is_some() {
            Style::default().fg(t.dim)
        } else {
            Style::default().fg(t.text)
        };
        let style = match (on, lit) {
            (true, true) if row.dim.is_some() => Style::default().fg(t.dim).bg(t.sel),
            (true, true) => wros_shell::look::highlight(look),
            (true, false) => base.bg(t.sel),
            _ => base,
        };
        let label_w = inner
            .width
            .saturating_sub(row.key.chars().count() as u16 + 5) as usize;
        let text = format!(
            "{}{:<label_w$}  {} ",
            if on && lit {
                wros_shell::look::MARKER
            } else {
                "  "
            },
            truncate(&row.label, label_w),
            row.key,
        );
        let r = Rect::new(inner.x, inner.y + line as u16, inner.width, 1);
        f.render_widget(Paragraph::new(Line::styled(text, style)), r);
        drawn.push((i, r));
    }
    Drawn {
        area: rect,
        rows: drawn,
    }
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
